// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What a structure still lacks for a level (the level's definition,
//! [`StructureType::detail`]), and advice: what reads but is worth a look
//! (shown, never counted in the level).

use ols_structures::{Detail, PoleRef, StructureType};

use crate::geometry::Handle;

/// Needed for the level, or advice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// Part of the level's definition: without it the level is not
    /// reached.
    Level,
    /// Worth a look; not counted in the level.
    Advice,
}

/// The table an item is found in (when it names no single row).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Points where strings hang.
    Points,
    /// Joints.
    Joints,
    /// Members.
    Members,
    /// Sections and materials.
    Sections,
    /// The pole (its outline, its design).
    Pole,
    /// The height extensions (their bodies and legs).
    Extensions,
}

/// One thing missing, or one piece of advice.
#[derive(Debug, Clone, PartialEq)]
pub struct Missing {
    /// Needed for the level, or advice.
    pub need: Need,
    /// In words.
    pub reason: String,
    /// The row it is about (picked when clicked), if one.
    pub target: Option<Handle>,
    /// The table it is found in.
    pub place: Place,
}

impl Missing {
    fn level(reason: impl Into<String>, target: Option<Handle>, place: Place) -> Self {
        Self {
            need: Need::Level,
            reason: reason.into(),
            target,
            place,
        }
    }

    fn advice(reason: impl Into<String>, target: Option<Handle>, place: Place) -> Self {
        Self {
            need: Need::Advice,
            reason: reason.into(),
            target,
            place,
        }
    }
}

/// Distance under which members are taken to cross, m.
const CROSSING: f64 = 1e-3;

/// What `t` lacks for `level` (1 to 3; nothing when it is there already),
/// then the advice: a point on no joint or member (it carries no load in
/// the analysis), a joint on one member (or none), members crossing
/// without a joint.
///
/// A pole (`[pole]`) has its own items: level 2 its outline, level 3 its
/// material, strength and wall (a thickness, solid or a section). At level
/// 3 an extension without members is advice (the model leaves it out), an extension's member
/// without a section or an extension body without a support a lack.
pub fn missing(t: &StructureType, level: u8) -> Vec<Missing> {
    let mut out = Vec::new();
    let now = t.detail().level();
    let pole = t.pole.as_ref().and_then(PoleRef::inline);
    if level >= 2 && now < 2 {
        let why = match (pole, t.body.is_empty()) {
            (Some(_), _) => {
                "no pole outline: diameters at two heights at least (Body › Pole), or members"
            }
            (None, true) => "no member: add joints and members (Body), or a 3D [[body]]",
            (None, false) => "the [[body]] polylines lie in one plane: members (Body) or a 3D body",
        };
        let place = if pole.is_some() {
            Place::Pole
        } else {
            Place::Members
        };
        out.push(Missing::level(why, None, place));
    }
    if level >= 3 && now < 3 {
        if let Some(p) = pole {
            for why in p.missing_for_design() {
                out.push(Missing::level(why, None, Place::Pole));
            }
        }
        if t.members.is_empty() && pole.is_some() {
            // The pole is the design.
        } else if t.members.is_empty() {
            if level == 3 && now == 2 {
                out.push(Missing::level(
                    "no member (a [[body]] is only drawn): joints and members (Body)",
                    None,
                    Place::Members,
                ));
            }
        } else {
            for m in t.members.iter().filter(|m| m.section.is_none()) {
                out.push(Missing::level(
                    format!("member {}: no section", m.id),
                    Some(Handle::Member(m.id.clone())),
                    Place::Members,
                ));
            }
            if !t.nodes.iter().any(|n| n.support.is_some()) {
                out.push(Missing::level(
                    "no supported joint: a support on the feet (Design)",
                    None,
                    Place::Joints,
                ));
            }
        }
    }
    if level >= 3 && !t.members.is_empty() {
        for e in &t.extensions {
            if !e.has_body() {
                out.push(Missing::advice(
                    format!(
                        "extension {} has no members: the FE model leaves it out",
                        e.name
                    ),
                    None,
                    Place::Extensions,
                ));
                continue;
            }
            for m in e.members.iter().filter(|m| m.section.is_none()) {
                out.push(Missing::level(
                    format!("extension {}: member {} has no section", e.name, m.id),
                    None,
                    Place::Extensions,
                ));
            }
            if !e.nodes.iter().any(|n| n.support.is_some()) {
                out.push(Missing::level(
                    format!("extension {}: no supported joint", e.name),
                    None,
                    Place::Extensions,
                ));
            }
        }
    }
    out.extend(advice(t));
    out
}

/// The advice for `t` (worth a look, never counted in the level).
fn advice(t: &StructureType) -> Vec<Missing> {
    let mut out = Vec::new();
    if t.members.is_empty() {
        return out;
    }
    for p in t
        .points
        .iter()
        .filter(|p| p.node.is_none() && p.member.is_none())
    {
        out.push(Missing::advice(
            format!(
                "point {} is on no joint or member: it carries no load in the analysis",
                p.label
            ),
            Some(Handle::Point(p.label.clone())),
            Place::Points,
        ));
    }
    for n in &t.nodes {
        let on = t.members.iter().filter(|m| m.nodes.contains(&n.id)).count();
        let why = match on {
            0 => "is on no member",
            1 => "is on one member only",
            _ => continue,
        };
        out.push(Missing::advice(
            format!("joint {} {why}", n.id),
            Some(Handle::Joint(n.id.clone())),
            Place::Joints,
        ));
    }
    let at = |id: &str| {
        t.nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.point.map(|v| v.si()))
    };
    // Each member with its two ends.
    let ends: Vec<(&ols_structures::structures::TypeMember, [[f64; 3]; 2])> = t
        .members
        .iter()
        .filter_map(|m| Some((m, [at(&m.nodes[0])?, at(&m.nodes[1])?])))
        .collect();
    for (i, (a, pa)) in ends.iter().enumerate() {
        for (b, pb) in &ends[i + 1..] {
            if a.nodes.iter().any(|n| b.nodes.contains(n)) {
                continue;
            }
            if crosses(pa[0], pa[1], pb[0], pb[1]) {
                out.push(Missing::advice(
                    format!("members {} and {} cross without a joint", a.id, b.id),
                    Some(Handle::Member(a.id.clone())),
                    Place::Members,
                ));
            }
        }
    }
    out
}

/// Whether segments `p0–p1` and `q0–q1` meet (within [`CROSSING`]) away
/// from their ends.
fn crosses(p0: [f64; 3], p1: [f64; 3], q0: [f64; 3], q1: [f64; 3]) -> bool {
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    // Boxes apart: no crossing (most pairs).
    for k in 0..3 {
        let (plo, phi) = (p0[k].min(p1[k]), p0[k].max(p1[k]));
        let (qlo, qhi) = (q0[k].min(q1[k]), q0[k].max(q1[k]));
        if phi + CROSSING < qlo || qhi + CROSSING < plo {
            return false;
        }
    }
    let (d1, d2, r) = (sub(p1, p0), sub(q1, q0), sub(p0, q0));
    let (a, e, f) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    let (b, c) = (dot(d1, d2), dot(d1, r));
    let denom = a * e - b * b;
    if a < 1e-12 || e < 1e-12 || denom <= 1e-12 * a * e {
        // A point, or parallel: overlapping along a line is not a
        // crossing.
        return false;
    }
    let s = (b * f - c * e) / denom;
    let u = (a * f - b * c) / denom;
    let inside = |x: f64, len: f64| {
        let margin = CROSSING / len.sqrt();
        x > margin && x < 1.0 - margin
    };
    if !inside(s, a) || !inside(u, e) {
        return false;
    }
    let gap = sub(
        [0, 1, 2].map(|k| p0[k] + s * d1[k]),
        [0, 1, 2].map(|k| q0[k] + u * d2[k]),
    );
    dot(gap, gap).sqrt() < CROSSING
}

/// The level after `d` (none above a full design).
pub fn next_level(d: Detail) -> Option<u8> {
    (d.level() < 3).then(|| d.level() + 1)
}
