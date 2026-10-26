// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Attachment points and the joints they are linked to (2026-10-21): the
//! joints of a placed structure, the rigid links to draw, a point linked
//! or unlinked, and the **matching** of a structure's points to the
//! joints of another type (a type swap, a revision that lost a joint).
//!
//! Matching is **exact** by default: a point matches a joint at the same
//! position (within [`ohl_structures::SAME_JOINT`]). Otherwise the
//! nearest attach joint is **offered**, the remaining distance kept as
//! the point's offset (whose moment the structure analysis applies); the
//! user chooses per point ([`PointChoice`]). No tolerance links a point
//! silently.

use ohl_structures::structures::{to_global, to_local};
use ohl_structures::{Joint, JointKind, StructureType};

use crate::assembly::distance;
use crate::{Point, Structure, StructurePoint};

/// Where a structure's type stands: its position raised by its height
/// adjustment (the type's feet).
pub fn feet(s: &Structure) -> Point {
    [
        s.position[0],
        s.position[1],
        s.position[2] + s.height_adjust,
    ]
}

/// A structure's axes in plan for its orientation (degrees CCW from
/// east of its local x): (right of the line, ahead), unit vectors in
/// line axes. The one place the convention is written (code review
/// 2026-10-26); [`to_structure_axes`] and [`from_structure_axes`] turn
/// vectors with it, `ohl_structures::structures::to_global` and
/// `to_local` points.
pub fn axes(orientation_deg: f64) -> ([f64; 2], [f64; 2]) {
    let a = orientation_deg.to_radians();
    let (s, c) = (a.sin(), a.cos());
    ([c, s], [-s, c])
}

/// Vector `v` (line axes) in the axes of a structure turned
/// `orientation_deg` ([`axes`]): (right, ahead, up).
pub fn to_structure_axes(orientation_deg: f64, v: Point) -> Point {
    let ([c, s], _) = axes(orientation_deg);
    [v[0] * c + v[1] * s, -v[0] * s + v[1] * c, v[2]]
}

/// Vector `local` (right, ahead, up) of a structure turned
/// `orientation_deg` in line axes ([`axes`]; the inverse of
/// [`to_structure_axes`]).
pub fn from_structure_axes(orientation_deg: f64, local: Point) -> Point {
    let ([c, s], _) = axes(orientation_deg);
    [
        local[0] * c - local[1] * s,
        local[0] * s + local[1] * c,
        local[2],
    ]
}

/// A joint of a placed structure's type, where it is.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedJoint {
    /// The joint ([`StructureType::joints`]).
    pub joint: Joint,
    /// Where it is, global, m (the structure's position, orientation,
    /// height adjustment and extension).
    pub position: Point,
}

/// The joints of type `t` as structure `s` places it (its position,
/// orientation, height adjustment and extension): what the Link tool
/// offers and draws.
pub fn placed_joints(t: &StructureType, s: &Structure) -> Vec<PlacedJoint> {
    // The type's joints listed once, lifted by the extension
    // ([`StructureType::joint_local`] for each).
    let Ok(dz) = t.lift(s.extension.as_deref()) else {
        return Vec::new();
    };
    t.joints()
        .into_iter()
        .map(|j| {
            let p = j.position;
            PlacedJoint {
                position: to_global(feet(s), s.orientation_deg, [p[0], p[1], p[2] + dz]),
                joint: j,
            }
        })
        .collect()
}

/// A rigid link to draw: from a joint to an attachment point at an offset
/// ([`rigid_links`]).
#[derive(Debug, Clone, PartialEq)]
pub struct RigidLink {
    /// The attachment point's label.
    pub label: String,
    /// The joint's id.
    pub joint: String,
    /// The joint, global, m.
    pub from: Point,
    /// The point, global, m.
    pub to: Point,
    /// The offset, structure axes, m.
    pub offset: Point,
    /// Its length, m.
    pub length: f64,
}

/// The rigid links of a resolved typed structure `s` (its type `t`):
/// every linked attachment point with an offset (none drawn for a point
/// on its joint).
pub fn rigid_links(t: &StructureType, s: &Structure) -> Vec<RigidLink> {
    // Most structures have none: no joints placed then (a lattice type
    // has thousands).
    if !s
        .points
        .iter()
        .any(|p| p.is_linked() && p.offset != [0.0; 3])
    {
        return Vec::new();
    }
    let joints = placed_joints(t, s);
    s.points
        .iter()
        .filter(|p| p.is_linked() && p.offset != [0.0; 3])
        .filter_map(|p| {
            let id = p.joint.as_deref()?;
            let j = joints.iter().find(|j| j.joint.id == id)?;
            Some(RigidLink {
                label: p.label.clone(),
                joint: id.to_owned(),
                from: j.position,
                to: p.position,
                offset: p.offset,
                length: p.offset_length(),
            })
        })
        .collect()
}

/// Attachment point `label` of structure `s` (type `t`) linked to joint
/// `joint` at `offset` (structure axes, m): the point to store as the
/// structure's own, its position the joint's as placed plus the offset.
/// The label and description are kept from `old` (the point as it was,
/// when there was one). Refused: no such joint, or a joint that is not
/// an attach joint (owner decision 2026-10-21: only attach joints take
/// links, [`ohl_structures::NOT_ATTACH`]).
pub fn linked(
    t: &StructureType,
    s: &Structure,
    old: Option<&StructurePoint>,
    label: &str,
    joint: &str,
    offset: Point,
) -> Result<StructurePoint, String> {
    let j = t
        .joint(joint)
        .ok_or_else(|| format!("{}: type {} has no joint {joint}", s.id, t.id))?;
    if j.kind == Some(JointKind::Support) {
        return Err(format!(
            "{}: joint {joint} is a support joint (strings hang from attach joints)",
            s.id
        ));
    }
    if j.kind != Some(JointKind::Attach) {
        return Err(format!(
            "{}: joint {joint}: {}",
            s.id,
            ohl_structures::NOT_ATTACH
        ));
    }
    let local = t
        .joint_local(joint, s.extension.as_deref())
        .ok_or_else(|| format!("{}: no extension {:?}", s.id, s.extension))?;
    let at = to_global(feet(s), s.orientation_deg, local);
    let o = to_global([0.0; 3], s.orientation_deg, offset);
    Ok(StructurePoint {
        description: old.map(|p| p.description.clone()).unwrap_or_default(),
        label: label.to_owned(),
        position: [at[0] + o[0], at[1] + o[1], at[2] + o[2]],
        joint: Some(joint.to_owned()),
        offset,
        unlinked: false,
    })
}

/// Attachment point `p` of a structure turned `rotation_deg` moved to
/// `to` (global, m): a linked point stays on its joint, its offset
/// changed by the move in the structure's axes (owner decision
/// 2026-10-21: the link follows edits); any other point only moves.
/// `p` is the point as resolved (its position its joint's plus its
/// offset).
pub fn moved(p: &StructurePoint, rotation_deg: f64, to: Point) -> StructurePoint {
    let mut out = StructurePoint {
        position: to,
        ..p.clone()
    };
    if p.is_linked() {
        let d = to_local(
            [0.0; 3],
            rotation_deg,
            [
                to[0] - p.position[0],
                to[1] - p.position[1],
                to[2] - p.position[2],
            ],
        );
        out.offset = [0, 1, 2].map(|k| {
            let v = p.offset[k] + d[k];
            if v.abs() < ohl_structures::SAME_JOINT {
                0.0
            } else {
                v
            }
        });
    }
    out
}

/// A typed structure `new` put in the place of `old` (both as stored):
/// each linked point of `new` that keeps its joint and offset but moved
/// in the structure's axes (from its feet, turned with it: a structure
/// moved or turned with its points is no move of theirs) stays on its
/// joint, its offset changed by the move ([`moved`]). Nothing is done
/// when the type, extension or height adjustment change (the joints
/// move then). Whether anything changed.
pub fn follow_moves(old: &Structure, new: &mut Structure) -> bool {
    if new.structure_type.is_none()
        || old.structure_type != new.structure_type
        || old.extension != new.extension
        || old.height_adjust != new.height_adjust
    {
        return false;
    }
    let mut changed = false;
    let (at, turned) = (feet(new), new.orientation_deg);
    for p in &mut new.points {
        let Some(q) = old.points.iter().find(|q| q.label == p.label) else {
            continue;
        };
        if !p.is_linked() || p.joint != q.joint || p.offset != q.offset {
            continue;
        }
        let a = to_local(feet(old), old.orientation_deg, q.position);
        let b = to_local(at, turned, p.position);
        if distance(a, b) <= crate::assembly::KEEP_POSITION {
            continue;
        }
        p.offset = [0, 1, 2].map(|k| {
            let v = q.offset[k] + b[k] - a[k];
            if v.abs() < ohl_structures::SAME_JOINT {
                0.0
            } else {
                v
            }
        });
        changed = true;
    }
    changed
}

/// Attachment point `p` unlinked: kept where it is, its structure's loads
/// refused until it is linked again.
pub fn unlinked(p: &StructurePoint) -> StructurePoint {
    StructurePoint {
        joint: None,
        offset: [0.0; 3],
        unlinked: true,
        ..p.clone()
    }
}

/// A joint offered for an attachment point: the nearest attach joint,
/// the point keeping its place at an offset from it.
#[derive(Debug, Clone, PartialEq)]
pub struct JointOffer {
    /// The joint's id.
    pub joint: String,
    /// From the joint to where the point is, structure axes, m.
    pub offset: Point,
    /// The offset's length, m.
    pub distance: f64,
}

/// How an attachment point matches the joints of a type ([`match_points`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PointMatch {
    /// The point's label.
    pub label: String,
    /// Where it is, global, m.
    pub position: Point,
    /// The joint at its position (none: no joint there).
    pub exact: Option<String>,
    /// Otherwise: the nearest attach joint, offered (none: no attach
    /// joint, or two equally near — [`PointMatch::tie`]).
    pub nearest: Option<JointOffer>,
    /// Two or more attach joints equally near (within
    /// [`ohl_structures::SAME_JOINT`]): none offered, the user chooses.
    pub tie: bool,
}

impl PointMatch {
    /// What is done without asking: the exact match, linked at no offset.
    /// None: the user chooses (the offer, another joint, or removal).
    pub fn proposal(&self) -> Option<PointChoice> {
        self.exact.as_ref().map(|j| PointChoice::Link {
            joint: j.clone(),
            offset: [0.0; 3],
        })
    }

    /// The offer taken: linked to the nearest attach joint at its offset.
    pub fn offer(&self) -> Option<PointChoice> {
        self.nearest.as_ref().map(|o| PointChoice::Link {
            joint: o.joint.clone(),
            offset: o.offset,
        })
    }
}

/// What becomes of an attachment point when its structure changes.
#[derive(Debug, Clone, PartialEq)]
pub enum PointChoice {
    /// Linked to this joint at this offset (structure axes, m).
    Link {
        /// The joint's id.
        joint: String,
        /// The offset.
        offset: Point,
    },
    /// Removed, with what hangs only from it.
    Remove,
}

/// The attachment points `points` (global positions, as a resolved
/// structure has them) matched to the joints of type `t` placed as
/// structure `s` says (its position, orientation, height adjustment and
/// extension — of the new type, for a swap): per point, the attach joint
/// at its position (exact), else the nearest attach joint offered with
/// the remaining distance as its offset (structure axes). Only attach
/// joints take links (owner decision 2026-10-21).
pub fn match_points(
    t: &StructureType,
    s: &Structure,
    points: &[StructurePoint],
) -> Vec<PointMatch> {
    let joints = placed_joints(t, s);
    points
        .iter()
        .map(|p| {
            let exact = joints
                .iter()
                .filter(|j| j.joint.kind == Some(JointKind::Attach))
                .find(|j| distance(j.position, p.position) <= ohl_structures::SAME_JOINT)
                .map(|j| j.joint.id.clone());
            let mut near: Vec<(f64, &PlacedJoint)> = joints
                .iter()
                .filter(|j| j.joint.kind == Some(JointKind::Attach))
                .map(|j| (distance(j.position, p.position), j))
                .collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            let tie = exact.is_none()
                && near.len() > 1
                && near[1].0 - near[0].0 <= ohl_structures::SAME_JOINT;
            let nearest = if exact.is_some() || tie {
                None
            } else {
                near.first().map(|(d, j)| {
                    let g = [
                        p.position[0] - j.position[0],
                        p.position[1] - j.position[1],
                        p.position[2] - j.position[2],
                    ];
                    JointOffer {
                        joint: j.joint.id.clone(),
                        offset: to_local([0.0; 3], s.orientation_deg, g),
                        distance: *d,
                    }
                })
            };
            PointMatch {
                label: p.label.clone(),
                position: p.position,
                exact,
                nearest,
                tie,
            }
        })
        .collect()
}

/// Structure `new` (as stored: a typed structure's own items) with the
/// attachment points of `old` (as resolved) given their `choices`: a
/// point linked as its new type `t` already links its label is left to
/// the type; any other linked point becomes the structure's own
/// ([`linked`]), and the old structure's own strings hanging from it
/// follow when their wire points are on the new structure. A point not
/// chosen, or removed, is dropped. Returns the structure and, in words,
/// what was dropped.
pub fn with_points(
    t: &StructureType,
    new: &Structure,
    old: &Structure,
    old_stored: &Structure,
    choices: &[(String, PointChoice)],
) -> Result<(Structure, Vec<String>), String> {
    let mut out = new.clone();
    let mut dropped = Vec::new();
    for p in &old.points {
        let choice = choices.iter().find(|(l, _)| *l == p.label).map(|(_, c)| c);
        match choice {
            Some(PointChoice::Link { joint, offset }) => {
                let by_type = t
                    .point_link(&p.label)
                    .is_some_and(|(j, o)| j == joint && o == *offset);
                if by_type {
                    continue;
                }
                out.points.retain(|q| q.label != p.label);
                out.points
                    .push(linked(t, &out, Some(p), &p.label, joint, *offset)?);
            }
            Some(PointChoice::Remove) | None => {
                dropped.push(format!("attachment point {}", p.label));
            }
        }
    }
    // The old structure's own strings from points kept, when their wire
    // points are there.
    let kept =
        |label: &str| out.points.iter().any(|q| q.label == label) || t.point_link(label).is_some();
    let wires = |label: &str| {
        out.wires.iter().any(|w| w.label == label) || t.wires.iter().any(|w| w.label == label)
    };
    for x in &old_stored.strings {
        if out.strings.iter().any(|y| y.label == x.label) {
            continue;
        }
        if kept(&x.top) && wires(&x.wire) {
            out.strings.push(x.clone());
        }
    }
    Ok((out, dropped))
}
