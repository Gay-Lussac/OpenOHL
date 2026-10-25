// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What a structure still lacks for a level (the level's definition,
//! [`StructureType::detail`]), and advice: what reads but is worth a look
//! (shown, never counted in the level).

use ohl_structures::{Detail, PoleRef, StructureType};

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
            (None, true) => {
                "no member: build the body (Body › Generators › legs and panels), or add joints \
                 and members"
            }
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
            let bare: Vec<_> = t.members.iter().filter(|m| m.section.is_none()).collect();
            if bare.len() > 3 {
                // Many (a body just built): one line, set per group.
                let groups: std::collections::BTreeSet<&str> =
                    bare.iter().filter_map(|m| m.group.as_deref()).collect();
                out.push(Missing::level(
                    format!(
                        "{} members without a cross-section{}: set them per group (Design › \
                         Groups)",
                        bare.len(),
                        if groups.is_empty() {
                            String::new()
                        } else {
                            format!(", in {} group(s)", groups.len())
                        }
                    ),
                    None,
                    Place::Members,
                ));
            } else {
                for m in bare {
                    out.push(Missing::level(
                        format!("member {}: no cross-section", m.id),
                        Some(Handle::Member(m.id.clone())),
                        Place::Members,
                    ));
                }
            }
            if !t.nodes.iter().any(|n| n.support.is_some()) {
                out.push(Missing::level(
                    "no supported joint: Support the feet (Design › Joints)",
                    None,
                    Place::Joints,
                ));
            }
            // As the level asks (`StructureType::missing_for_design`):
            // every attachment point on a joint of the members, every
            // support joint supported.
            let joints = t.joints();
            for p in &t.points {
                if joints
                    .iter()
                    .any(|j| j.id == p.joint && j.source == ohl_structures::JointSource::Point)
                {
                    out.push(Missing::level(
                        format!(
                            "point {} is on no joint of the members: tick it in the peak or \
                             crossarm generator, or link it to a joint (Link tool)",
                            p.label
                        ),
                        Some(Handle::Point(p.label.clone())),
                        Place::Points,
                    ));
                }
            }
            for n in &t.nodes {
                if n.kind == Some(ohl_structures::JointKind::Support) && n.support.is_none() {
                    out.push(Missing::level(
                        format!("joint {} is a support joint without a support", n.id),
                        Some(Handle::Joint(n.id.clone())),
                        Place::Joints,
                    ));
                }
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
                    format!("extension {}: member {} has no cross-section", e.name, m.id),
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
    // The members meeting each joint id (a count per joint through every
    // member was slow on a large lattice).
    let mut meeting: std::collections::HashMap<&str, usize> = Default::default();
    for m in &t.members {
        for (k, id) in m.nodes.iter().enumerate() {
            if !m.nodes[..k].contains(id) {
                *meeting.entry(id.as_str()).or_default() += 1;
            }
        }
    }
    for n in &t.nodes {
        let on = meeting.get(n.id.as_str()).copied().unwrap_or(0);
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
    // The joints by id (the first of an id, as a search finds it).
    let mut joints: std::collections::HashMap<&str, [f64; 3]> = Default::default();
    for n in &t.nodes {
        joints
            .entry(n.id.as_str())
            .or_insert_with(|| n.point.map(|v| v.si()));
    }
    let at = |id: &str| joints.get(id).copied();
    // Each member with its two ends.
    let ends: Vec<(&ohl_structures::structures::TypeMember, [[f64; 3]; 2])> = t
        .members
        .iter()
        .filter_map(|m| Some((m, [at(&m.nodes[0])?, at(&m.nodes[1])?])))
        .collect();
    // Crossings within one group (the X panels a generator made): one
    // line per group; across groups, each pair.
    // The pairs whose heights overlap (a sweep up the members sorted by
    // their lowest point), then taken in the members' order as before.
    let low = |p: &[[f64; 3]; 2]| p[0][2].min(p[1][2]);
    let high = |p: &[[f64; 3]; 2]| p[0][2].max(p[1][2]);
    let mut by_low: Vec<usize> = (0..ends.len()).collect();
    by_low.sort_by(|&i, &j| low(&ends[i].1).total_cmp(&low(&ends[j].1)));
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for (k, &i) in by_low.iter().enumerate() {
        let top = high(&ends[i].1) + CROSSING;
        for &j in &by_low[k + 1..] {
            if low(&ends[j].1) > top {
                break;
            }
            pairs.push((i.min(j), i.max(j)));
        }
    }
    pairs.sort_unstable();
    let mut in_group: std::collections::BTreeMap<&str, (usize, &str)> = Default::default();
    for (i, j) in pairs {
        let ((a, pa), (b, pb)) = (&ends[i], &ends[j]);
        {
            if a.nodes.iter().any(|n| b.nodes.contains(n)) {
                continue;
            }
            if crosses(pa[0], pa[1], pb[0], pb[1]) {
                match (a.group.as_deref(), b.group.as_deref()) {
                    (Some(g), Some(h)) if g == h => {
                        in_group.entry(g).or_insert((0, a.id.as_str())).0 += 1;
                    }
                    _ => out.push(Missing::advice(
                        format!("members {} and {} cross without a joint", a.id, b.id),
                        Some(Handle::Member(a.id.clone())),
                        Place::Members,
                    )),
                }
            }
        }
    }
    for (g, (n, first)) in in_group {
        out.push(Missing::advice(
            if n == 1 {
                format!("two members of group {g} cross without a joint")
            } else {
                format!(
                    "group {g}: {n} crossings without a joint (X bracing bolted at the \
                     crossing is common)"
                )
            },
            Some(Handle::Member(first.to_owned())),
            Place::Members,
        ));
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
