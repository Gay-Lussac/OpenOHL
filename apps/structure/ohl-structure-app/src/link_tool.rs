// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The **Link tool** (Geometry › Link tool, 2026-10-21): click an
//! attachment point (where a string hangs), then a joint it may be linked
//! to: the point is linked to that joint where it is, its distance from
//! the joint kept as its offset (a rigid link, whose moment the structure
//! analysis applies at the joint) — one edit, one undo step. Relinking a
//! linked point is the same two clicks. A right-click or Escape cancels
//! (a second one leaves the tool). While a point is chosen the joints it
//! may be linked to (every joint but a support joint) are ringed; the one
//! under the pointer shows the offset's length and its moment arm. A
//! point is unlinked from its context menu (in the view or the Points
//! table): it keeps its place as a joint of its own.

use ohl_structure_edit::Handle;
use ohl_structures::StructureType;

use crate::app::StructureApp;

/// The Link tool, held.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LinkTool {
    /// The attachment point chosen (first click), waiting for its joint.
    pub point: Option<String>,
}

/// What the 3D view asks of the application about links.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkAction {
    /// Unlink point `label` (its context menu).
    Unlink(String),
    /// Start the Link tool on point `label` (its context menu).
    Start(String),
}

/// The joints attachment points may be linked to (all but support
/// joints), with their positions (local axes, m) and whether each is an
/// attach joint already.
pub fn linkable(t: &StructureType) -> Vec<(String, [f64; 3], bool)> {
    t.linkable_joints()
        .into_iter()
        .map(|j| {
            let attach = j.kind == Some(ohl_structures::JointKind::Attach);
            (j.id, j.position, attach)
        })
        .collect()
}

/// From joint `joint` to attachment point `label` of `t`: the offset it
/// would be linked at (where the point is), local axes, m.
pub fn offset_to(t: &StructureType, label: &str, joint: &str) -> Option<[f64; 3]> {
    let p = t.points.iter().find(|p| p.label == label)?.position();
    let j = t.joint(joint)?.position;
    Some([0, 1, 2].map(|k| p[k].si() - j[k]))
}

/// The offset's length and its moment arm for a vertical load (its
/// length across the vertical), m, in words.
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

impl StructureApp {
    /// Geometry › Link tool: held (or let go).
    pub fn toggle_link_tool(&mut self) {
        if self.link.take().is_some() {
            self.message = Some(("Link tool let go".into(), false));
        } else {
            self.link = Some(LinkTool::default());
            self.message = Some((
                "Link tool: click an attachment point, then the joint to link it to \
                 (right-click or Escape: cancel)"
                    .into(),
                false,
            ));
        }
    }

    /// Right-click or Escape with the Link tool: the point chosen is
    /// dropped; with none chosen, the tool is let go.
    pub fn cancel_link(&mut self) {
        match &mut self.link {
            Some(l) if l.point.is_some() => {
                l.point = None;
                self.message = Some(("Link: cancelled".into(), false));
            }
            Some(_) => {
                self.link = None;
                self.message = Some(("Link tool let go".into(), false));
            }
            None => {}
        }
    }

    /// A click with the Link tool on `h` (a point picked, or a joint the
    /// point may be linked to).
    pub fn link_click(&mut self, h: Option<Handle>) {
        let Some(tool) = &mut self.link else {
            return;
        };
        match (tool.point.clone(), h) {
            (_, Some(Handle::Point(l))) => {
                tool.point = Some(l.clone());
                self.selected = vec![Handle::Point(l.clone())];
                self.message = Some((
                    format!(
                        "Link {l}: click the joint to link it to (ringed; right-click or \
                         Escape: cancel)"
                    ),
                    false,
                ));
            }
            (Some(l), Some(Handle::Joint(j))) => self.link_point(&l, &j),
            (Some(_), _) => {
                self.message = Some((
                    "Link: click a ringed joint (a support joint takes no strings)".into(),
                    true,
                ));
            }
            (None, _) => {
                self.message = Some((
                    "Link: click an attachment point first (where a string hangs)".into(),
                    true,
                ));
            }
        }
    }

    /// Links point `label` to joint `joint` where it is (one edit).
    pub fn link_point(&mut self, label: &str, joint: &str) {
        let Some(f) = &mut self.file else {
            return;
        };
        let Some(offset) = offset_to(f.structure(), label, joint) else {
            self.message = Some((format!("no point {label} or joint {joint}"), true));
            return;
        };
        let offset = offset.map(|v| {
            if v.abs() < ohl_structures::SAME_JOINT {
                0.0
            } else {
                v
            }
        });
        let what = format!("link point {label} to joint {joint}");
        self.message = Some(
            match f.edit(&what, |doc, t| {
                ohl_structure_edit::links::link_point(doc, t, label, joint, offset)
            }) {
                Ok(()) => (format!("{what}: {}", offset_words(offset)), false),
                Err(e) => (e, true),
            },
        );
        if let Some(l) = &mut self.link {
            l.point = None;
        }
        self.selected = vec![Handle::Point(label.to_owned())];
    }

    /// Unlinks point `label`: it keeps its place, a joint of its own (one
    /// edit).
    pub fn unlink_point(&mut self, label: &str) {
        let Some(f) = &mut self.file else {
            return;
        };
        let what = format!("unlink point {label}");
        self.message = Some(
            match f.edit(&what, |doc, t| {
                ohl_structure_edit::links::unlink_point(doc, t, label)
            }) {
                Ok(()) => (
                    format!("{what}: it keeps its place, a joint of its own on no member"),
                    false,
                ),
                Err(e) => (e, true),
            },
        );
    }

    /// What the view asked about links.
    pub fn link_action(&mut self, a: LinkAction) {
        match a {
            LinkAction::Unlink(label) => self.unlink_point(&label),
            LinkAction::Start(label) => {
                self.link = Some(LinkTool::default());
                self.link_click(Some(Handle::Point(label)));
            }
        }
    }

    /// Make the members meeting joint `joint` frames (the offset moment of
    /// a point there is then carried): one edit; their sections need a
    /// bending stiffness (iy, iz, torsion).
    pub fn make_frame_joint(&mut self, joint: &str) {
        let Some(f) = &mut self.file else {
            return;
        };
        let members: Vec<String> = f
            .structure()
            .members
            .iter()
            .filter(|m| m.nodes.iter().any(|n| n == joint))
            .map(|m| m.id.clone())
            .collect();
        if members.is_empty() {
            self.message = Some((format!("no member meets joint {joint}"), true));
            return;
        }
        let what = format!("frame joint {joint}");
        f.edit(&what, |doc, _| {
            for m in &members {
                ohl_structure_edit::set_member_kind(doc, m, "frame");
            }
        });
        self.message = Some((
            format!(
                "{what}: members {} made frames (their sections need iy, iz and torsion); run \
                 the results again",
                members.join(", ")
            ),
            false,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_words_give_length_and_arm() {
        assert_eq!(offset_words([0.0; 3]), "on it (no offset)");
        let w = offset_words([0.3, 0.4, 1.0]);
        assert!(w.contains("0.500 m for a vertical load"), "{w}");
        assert!(w.contains("1.118 m long"), "{w}");
    }
}
