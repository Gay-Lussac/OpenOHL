// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Attachment points linked to joints in the line (2026-10-21): the
//! **Link tool** ([`crate::tools::Tool::Link`]), the **rigid links**
//! drawn, the point's facts for Properties.
//!
//! - **Link tool**: the tool click (a click with the tool held, or Link
//!   to a joint… in the point's right-click menu) on an attachment point of a typed
//!   structure chooses it; the attach joints of its type are ringed in
//!   the 3D view (only attach joints take links: owner decision
//!   2026-10-21), the one under the pointer showing the offset's length
//!   and its moment arm; the tool click on one links the point there
//!   where it is, the distance kept as its offset
//!   ([`ohl_session::Command::LinkPoint`]: one command, one undo).
//!   Another joint is neither ringed nor clickable: under the pointer it
//!   says "not an attach joint: set its kind to attach in OHL-Structure".
//!   Relinking a linked point is the same. Escape, or the tool click
//!   elsewhere, cancels. A point is unlinked from
//!   Properties (Unlink), the command
//!   [`ohl_session::Command::UnlinkPoint`].
//! - **Rigid links** ([`crate::scene::Layer::RigidLinks`]): a solid rod
//!   from the joint to each attachment point at an offset (none at zero
//!   offset), in its own colour, on its own layer; a click on one selects
//!   the attachment point, Properties shows the joint, the offset vector,
//!   its length and its moment per load case (when Checks › Structure
//!   loads has computed them).
//! - **Unlinked points** (their joint gone from the type, or unlinked)
//!   are drawn red: their structure's loads are refused until they are
//!   linked again.

use ohl_line::links::{PlacedJoint, linked, placed_joints, rigid_links, unlinked};
use ohl_line::{Structure, StructurePoint};
use ohl_session::Command;
use ohl_structures::JointKind;
use ohl_structures::structures::to_local;

use crate::model::Model;

/// The colour of the rigid links (magenta, apart from the steel and the
/// strings).
pub const RIGID: [u8; 4] = [230, 80, 220, 255];

/// The colour of an unlinked attachment point (red).
pub const UNLINKED: [u8; 4] = [240, 50, 50, 255];

/// A joint of the type, as the Link tool sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkJoint {
    /// Its id.
    pub id: String,
    /// Where it is, global, m.
    pub at: [f64; 3],
    /// An attach joint: ringed, a point may be linked to it. Else (a
    /// joint without a kind) only its hover says why not.
    pub attach: bool,
}

/// The Link tool's state in the 3D view: the point chosen, the joints it
/// may be linked to, the one under the pointer and the one clicked
/// (taken by the application).
#[derive(Debug, Clone, PartialEq)]
pub struct LinkOverlay {
    /// The structure.
    pub structure: String,
    /// The attachment point chosen.
    pub label: String,
    /// Where it is, global, m.
    pub point: [f64; 3],
    /// Its structure's orientation, degrees (the offset in its axes).
    pub orientation_deg: f64,
    /// The joints of its type but support joints (attach joints ringed
    /// and clickable).
    pub joints: Vec<LinkJoint>,
    /// The joint under the pointer (any of [`LinkOverlay::joints`]).
    pub hovered: Option<String>,
    /// The attach joint clicked with the tool.
    pub clicked: Option<String>,
}

impl LinkOverlay {
    /// From the point to joint `id`: the offset it would be linked at, in
    /// the structure's axes, m.
    pub fn offset_to(&self, id: &str) -> Option<[f64; 3]> {
        let j = self.joints.iter().find(|j| j.id == id)?;
        Some(offset(self.orientation_deg, j.at, self.point))
    }

    /// What the joint under the pointer says: the link it would make, or
    /// (not an attach joint) why it makes none.
    pub fn hover_text(&self) -> Option<String> {
        let id = self.hovered.as_ref()?;
        let j = self.joints.iter().find(|j| &j.id == id)?;
        if !j.attach {
            return Some(format!("joint {id}: {}", ohl_structures::NOT_ATTACH));
        }
        let o = self.offset_to(id)?;
        Some(format!(
            "link {} of {} to joint {id}: {}",
            self.label,
            self.structure,
            offset_words(o)
        ))
    }

    /// The tool click on the joint under the pointer: an attach joint is
    /// clicked; another one is not (none).
    pub fn click(&mut self) -> Option<String> {
        let id = self.hovered.clone()?;
        let attach = self.joints.iter().any(|j| j.id == id && j.attach);
        attach.then(|| {
            self.clicked = Some(id.clone());
            id
        })
    }
}

/// The offset from `joint` to `point` (global) in the axes of a structure
/// turned `orientation_deg`, rounding noise taken off.
pub fn offset(orientation_deg: f64, joint: [f64; 3], point: [f64; 3]) -> [f64; 3] {
    let d = [0, 1, 2].map(|k| point[k] - joint[k]);
    to_local([0.0; 3], orientation_deg, d).map(|v| {
        if v.abs() < ohl_structures::SAME_JOINT {
            0.0
        } else {
            v
        }
    })
}

/// The offset in words: its components, its length and its moment arm
/// for a vertical load.
pub fn offset_words(o: [f64; 3]) -> String {
    let length = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt();
    if length < ohl_structures::SAME_JOINT {
        return "on it (no offset)".into();
    }
    format!(
        "offset ({:.3}, {:.3}, {:.3}) m, {:.3} m long; moment arm {:.3} m for a vertical load",
        o[0],
        o[1],
        o[2],
        length,
        o[0].hypot(o[1])
    )
}

/// Structure `id` (as resolved) and its type, when it has one in the
/// library.
fn typed<'a>(
    model: &'a Model,
    id: &str,
) -> Result<(&'a Structure, &'a ohl_structures::StructureType), String> {
    let s = model
        .line
        .structure(id)
        .ok_or_else(|| format!("no structure {id}"))?;
    let t = s
        .structure_type
        .as_deref()
        .ok_or_else(|| format!("{id} has no type: its attachment points are their own joints"))?;
    let t = model
        .types
        .as_ref()
        .and_then(|(types, _)| types.get(t))
        .ok_or_else(|| format!("{id}: type {t} not in the library"))?;
    Ok((s, t))
}

/// The joints of structure `id`'s type but support joints, where they
/// are, each marked whether it is an attach joint (only those take a
/// link).
pub fn joints(model: &Model, id: &str) -> Vec<LinkJoint> {
    let Ok((s, t)) = typed(model, id) else {
        return Vec::new();
    };
    placed_joints(t, s)
        .into_iter()
        .filter(|j: &PlacedJoint| j.joint.kind != Some(JointKind::Support))
        .map(|j| LinkJoint {
            attach: j.joint.kind == Some(JointKind::Attach),
            id: j.joint.id,
            at: j.position,
        })
        .collect()
}

/// The attach joints of structure `id`'s type (only these take strings),
/// with where they are in the structure's axes (from its position, as an
/// editor puts its points); none for a structure without a type.
pub fn attach_joints(model: &Model, id: &str) -> Vec<(String, [f64; 3])> {
    let Ok((s, t)) = typed(model, id) else {
        return Vec::new();
    };
    placed_joints(t, s)
        .into_iter()
        .filter(|j| j.joint.kind == Some(JointKind::Attach))
        .map(|j| {
            (
                j.joint.id,
                to_local(s.position, s.orientation_deg, j.position),
            )
        })
        .collect()
}

/// The Link tool's overlay for attachment point `label` of structure
/// `id`: refused for a wire point or a structure without a type.
pub fn overlay(model: &Model, id: &str, label: &str) -> Result<LinkOverlay, String> {
    let (s, _) = typed(model, id)?;
    let p = s.point(label).ok_or_else(|| {
        format!("{id} {label}: not an attachment point (a wire point is not linked to a joint)")
    })?;
    Ok(LinkOverlay {
        structure: id.to_owned(),
        label: label.to_owned(),
        point: p.position,
        orientation_deg: s.orientation_deg,
        joints: joints(model, id),
        hovered: None,
        clicked: None,
    })
}

/// The command linking attachment point `label` of structure `id` to
/// joint `joint` where the point is (its distance kept as the offset),
/// with what it does in words.
pub fn link_command(
    model: &Model,
    id: &str,
    label: &str,
    joint: &str,
) -> Result<(String, Command), String> {
    model.can_edit()?;
    let (s, t) = typed(model, id)?;
    let old = s
        .point(label)
        .ok_or_else(|| format!("{id} has no attachment point {label}"))?;
    let j = placed_joints(t, s)
        .into_iter()
        .find(|j| j.joint.id == joint)
        .ok_or_else(|| format!("{id}: type {} has no joint {joint}", t.id))?;
    let o = offset(s.orientation_deg, j.position, old.position);
    link_at(model, id, label, joint, o)
}

/// The command linking attachment point `label` of structure `id` to
/// joint `joint` at offset `o` (structure axes, m).
pub fn link_at(
    model: &Model,
    id: &str,
    label: &str,
    joint: &str,
    o: [f64; 3],
) -> Result<(String, Command), String> {
    model.can_edit()?;
    let (s, t) = typed(model, id)?;
    let old = s.point(label);
    let p = linked(t, s, old, label, joint, o)?;
    let relink = old
        .and_then(|p| p.joint.as_deref())
        .is_some_and(|j| j != joint);
    Ok((
        format!(
            "{} attachment point {label} of {id} to joint {joint}: {}",
            if relink { "relinked" } else { "linked" },
            offset_words(o)
        ),
        Command::LinkPoint {
            structure: id.to_owned(),
            label: label.to_owned(),
            joint: joint.to_owned(),
            offset: p.offset,
            position: p.position,
        },
    ))
}

/// The command unlinking attachment point `label` of structure `id`: it
/// stays where it is, its structure's loads refused until it is linked
/// again.
pub fn unlink_command(model: &Model, id: &str, label: &str) -> Result<(String, Command), String> {
    model.can_edit()?;
    let (s, _) = typed(model, id)?;
    let p = s
        .point(label)
        .ok_or_else(|| format!("{id} has no attachment point {label}"))?;
    let u = unlinked(p);
    Ok((
        format!(
            "unlinked attachment point {label} of {id}: it stays where it is, the structure's \
             loads are refused until it is linked again"
        ),
        Command::UnlinkPoint {
            structure: id.to_owned(),
            label: label.to_owned(),
            position: u.position,
        },
    ))
}

/// The rigid links of every typed structure of the line: (structure,
/// label, from the joint, to the point).
pub fn drawn_links(model: &Model) -> Vec<(String, String, [f64; 3], [f64; 3])> {
    let Some((types, _)) = &model.types else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for s in &model.line.structures {
        let Some(t) = s.structure_type.as_deref().and_then(|t| types.get(t)) else {
            continue;
        };
        for l in rigid_links(t, s) {
            out.push((s.id.clone(), l.label, l.from, l.to));
        }
    }
    out
}

/// The unlinked attachment points of the line: (structure, label, where).
pub fn unlinked_points(line: &ohl_line::Line) -> Vec<(String, String, [f64; 3])> {
    line.structures
        .iter()
        .filter(|s| s.structure_type.is_some())
        .flat_map(|s| {
            s.points
                .iter()
                .filter(|p| p.unlinked)
                .map(|p| (s.id.clone(), p.label.clone(), p.position))
        })
        .collect()
}

/// The facts of attachment point `p` of structure `s` for Properties: its
/// joint (or unlinked), the offset vector and length, and the moment of
/// the offset per load case of `loads` (r × F of its factored force),
/// where computed.
pub fn point_rows(
    s: &Structure,
    p: &StructurePoint,
    loads: Option<&[ohl_loads::CaseLoads]>,
) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    if s.structure_type.is_none() {
        rows.push((
            "joint".to_owned(),
            "its own (a structure without a type)".to_owned(),
        ));
        return rows;
    }
    if p.unlinked {
        rows.push((
            "joint".to_owned(),
            "unlinked: the structure's loads are refused until it is linked again (Link tool)"
                .to_owned(),
        ));
        return rows;
    }
    if p.joint.is_none() {
        rows.push((
            "joint".to_owned(),
            "none named: its type gives it as an attachment (version 1), found by it".to_owned(),
        ));
        return rows;
    }
    rows.push(("joint".to_owned(), p.joint.clone().unwrap_or_default()));
    let o = p.offset;
    rows.push((
        "offset (x, y, z)".to_owned(),
        format!("{:.3}, {:.3}, {:.3} m", o[0], o[1], o[2]),
    ));
    rows.push((
        "offset length".to_owned(),
        format!("{:.3} m", p.offset_length()),
    ));
    if p.offset == [0.0; 3] {
        return rows;
    }
    for c in loads.unwrap_or_default() {
        let f = c
            .loads
            .iter()
            .filter(|l| l.structure == s.id && l.point == p.label)
            .fold([0.0; 3], |a, l| {
                // Structure axes, z up: transverse, longitudinal, −down.
                [
                    a[0] + l.factored[0],
                    a[1] + l.factored[1],
                    a[2] - l.factored[2],
                ]
            });
        if f == [0.0; 3] {
            continue;
        }
        let m = [
            o[1] * f[2] - o[2] * f[1],
            o[2] * f[0] - o[0] * f[2],
            o[0] * f[1] - o[1] * f[0],
        ];
        let size = (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt();
        rows.push((
            format!("moment, {}", c.case.name),
            format!(
                "{:.2} kN·m ({:.2}, {:.2}, {:.2})",
                size / 1000.0,
                m[0] / 1000.0,
                m[1] / 1000.0,
                m[2] / 1000.0
            ),
        ));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_in_the_structures_axes_and_in_words() {
        // A structure turned 90°: global +x is its −y (or +y): the length
        // is kept, rounding taken off.
        let o = offset(90.0, [10.0, 0.0, 5.0], [10.0, 0.5, 5.0]);
        assert!(((o[0] * o[0] + o[1] * o[1]).sqrt() - 0.5).abs() < 1e-12);
        assert_eq!(o[2], 0.0);
        assert_eq!(offset_words([0.0; 3]), "on it (no offset)");
        let w = offset_words([0.3, 0.4, 0.0]);
        assert!(w.contains("0.500 m long"), "{w}");
        assert!(w.contains("moment arm 0.500 m"), "{w}");
    }
}
