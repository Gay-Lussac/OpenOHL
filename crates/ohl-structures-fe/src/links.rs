// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Attachment points linked to joints (2026-10-21): where a point's load
//! acts in the model and the moment of its offset (a rigid link).
//!
//! - A point on a node: its force at the node; with an offset r (from the
//!   joint to the point) also the moment r × F where a frame member meets
//!   the node. Where only trusses (cables, tension-only members) meet it
//!   the moment cannot be carried: it is left out and said so
//!   ([`PointLoad::note`]) — make the joint a frame joint to carry it.
//! - A point cannot be put on a member (owner decision 2026-10-21): a
//!   structure file that places one along a member (`member`, `at`) is
//!   read with a joint inserted there and the member split into
//!   continuous pieces ([`ohl_structures::on_members`]), so the point is
//!   on a node like any other. The share of its force to the member's
//!   ends is gone.
//! - A node carrying a point where only bars (trusses, cables,
//!   tension-only members) meet, all in a line or a plane, cannot carry a
//!   load across them: the solve is refused with the fix
//!   ([`Model::bar_joint_error`]) instead of holding the joint
//!   (stabilisation) and so sharing the load to the bars' ends.

use ohl_core::units::Length;
use serde::Deserialize;

use crate::model::{JointPoint, Model, NodalLoad, cross};

/// Where an attachment point's load goes in the model.
#[derive(Debug, Clone, PartialEq)]
pub struct PointLoad {
    /// The loads at the nodes.
    pub loads: Vec<NodalLoad>,
    /// The moment of the point's offset, r × F, N·m (structure axes; zero
    /// without an offset).
    pub moment: [f64; 3],
    /// Whether that moment is in [`PointLoad::loads`] (a frame member
    /// carries it).
    pub moment_applied: bool,
    /// What was left out, in words (none: nothing).
    pub note: Option<String>,
}

fn is_zero(v: [f64; 3]) -> bool {
    v == [0.0; 3]
}

fn kn_m(m: [f64; 3]) -> f64 {
    (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt() / 1000.0
}

/// A load across the bars at a joint smaller than this share of the
/// joint's load (and than [`ACROSS_N`]) is rounding only.
const ACROSS: f64 = 1e-9;

/// See [`ACROSS`], N.
const ACROSS_N: f64 = 1e-3;

impl Model {
    /// The points of a structure file's `[[point]]` tables linked to the
    /// model's nodes, by the structure's rule
    /// ([`ohl_structures::point_joints`]): a point on a node (or at a
    /// node's position, or named by a node's `attachment`) is a
    /// [`JointPoint`]; a point at a position of its own on no node has no
    /// place in the model (its load is refused: [`Model::point_load`]
    /// gives none). Points written along a member are on the joints
    /// inserted there when the file's tables are read
    /// ([`ohl_structures::on_members::apply_to_table`]).
    pub(crate) fn link_points(&mut self, points: Option<&Vec<toml::Value>>) -> Result<(), String> {
        let length3 = |v: &toml::Value, what: &str, label: &str| {
            <[Length; 3]>::deserialize(v.clone())
                .map(|l| l.map(Length::si))
                .map_err(|e| format!("point {label}: {what}: {e}"))
        };
        let mut rows = Vec::new();
        for p in points.into_iter().flatten() {
            let Some(label) = p.get("label").and_then(toml::Value::as_str) else {
                continue;
            };
            let offset = match p.get("offset") {
                Some(v) => length3(v, "offset", label)?,
                None => [0.0; 3],
            };
            if p.get("member").is_some() || p.get("at").is_some() {
                return Err(format!(
                    "point {label}: a point is on a joint, not along a member (insert a joint \
                     on the member)"
                ));
            }
            let place = match p.get("node").and_then(toml::Value::as_str) {
                Some(n) => ohl_structures::PointPlace::Joint(n),
                None => match p.get("point") {
                    Some(v) => ohl_structures::PointPlace::Free(length3(v, "point", label)?),
                    None => continue,
                },
            };
            rows.push((label, place, offset));
        }
        let joints: Vec<(&str, [f64; 3], Option<&str>)> = self
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.point, n.attachment.as_deref()))
            .collect();
        let places: Vec<_> = rows.iter().map(|(l, p, _)| (*l, *p)).collect();
        let ids = ohl_structures::point_joints(&joints, &places);
        let mut joint_points = Vec::new();
        for ((label, place, offset), joint) in rows.iter().zip(ids) {
            let node = |id: &str| self.nodes.iter().position(|n| n.id == id);
            match *place {
                ohl_structures::PointPlace::Joint(id) => {
                    let n = node(id).ok_or_else(|| format!("point {label}: no joint {id}"))?;
                    joint_points.push(JointPoint {
                        label: (*label).to_owned(),
                        node: n,
                        offset: *offset,
                    });
                }
                ohl_structures::PointPlace::Free(at) => {
                    let Some(n) = node(&joint) else {
                        continue;
                    };
                    let q = self.nodes[n].point;
                    let d = [at[0] - q[0], at[1] - q[1], at[2] - q[2]];
                    let far = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                        > ohl_structures::SAME_JOINT;
                    joint_points.push(JointPoint {
                        label: (*label).to_owned(),
                        node: n,
                        offset: if far { d } else { [0.0; 3] },
                    });
                }
            }
        }
        // A node names the first point on it (as before the links).
        for p in &joint_points {
            let n = &mut self.nodes[p.node];
            if n.attachment.is_none() {
                n.attachment = Some(p.label.clone());
            }
        }
        self.joint_points = joint_points;
        Ok(())
    }

    /// The node carrying the structure's point labelled `label`: a point on
    /// a node, or a separate model's node naming it
    /// ([`crate::Node::attachment`]).
    pub fn attachment_node(&self, label: &str) -> Option<usize> {
        self.place_of(label).map(|(n, _)| n)
    }

    /// The node of the point labelled `label`, and its offset.
    fn place_of(&self, label: &str) -> Option<(usize, [f64; 3])> {
        if let Some(p) = self.joint_points.iter().find(|p| p.label == label) {
            return Some((p.node, p.offset));
        }
        self.nodes
            .iter()
            .position(|n| n.attachment.as_deref() == Some(label))
            .map(|n| (n, [0.0; 3]))
    }

    /// The load of the point labelled `label` with `force` (N, structure
    /// axes), linked as the structure file (or the separate model's
    /// node) says. None: no such point in the model.
    pub fn point_load(&self, label: &str, force: [f64; 3]) -> Option<PointLoad> {
        let (node, offset) = self.place_of(label)?;
        Some(self.load_at(label, node, offset, force))
    }

    /// The load of point `label` linked to joint `joint` at `offset`
    /// (structure axes, m) with `force` (N): the link a placed structure
    /// gives (the line's attachment point: its own relink wins over the
    /// type's). The joint is a node of the model; a separate model
    /// (`.ohlfe`, its own node ids) is looked up by the point's label,
    /// the offset kept. None: neither.
    pub fn linked_load(
        &self,
        label: &str,
        joint: &str,
        offset: [f64; 3],
        force: [f64; 3],
    ) -> Option<PointLoad> {
        let node = self
            .nodes
            .iter()
            .position(|n| n.id == joint)
            .or_else(|| self.place_of(label).map(|(n, _)| n))?;
        Some(self.load_at(label, node, offset, force))
    }

    /// The loads at the structure's point labelled `label` with `force`
    /// (N, structure axes): [`Model::point_load`]'s loads. None: no such
    /// point in the model.
    pub fn point_loads(&self, label: &str, force: [f64; 3]) -> Option<Vec<NodalLoad>> {
        self.point_load(label, force).map(|p| p.loads)
    }

    /// Whether a frame member meets node `n`.
    pub fn frame_at(&self, n: usize) -> bool {
        self.members
            .iter()
            .any(|m| (m.i == n || m.j == n) && m.is_frame())
    }

    fn load_at(&self, label: &str, n: usize, offset: [f64; 3], force: [f64; 3]) -> PointLoad {
        let moment = if is_zero(offset) {
            [0.0; 3]
        } else {
            cross(offset, force)
        };
        let some = !is_zero(moment);
        let frame = self.frame_at(n);
        PointLoad {
            loads: vec![NodalLoad {
                node: n,
                force,
                moment: if frame && some { moment } else { [0.0; 3] },
            }],
            moment,
            moment_applied: frame && some,
            note: (some && !frame).then(|| {
                format!(
                    "attachment point {label}: the moment of its offset ({:.2} kN m) left out \
                     at joint {}: only trusses meet it (make it a frame joint to carry it)",
                    kn_m(moment),
                    self.nodes[n].id
                )
            }),
        }
    }

    /// The directions node `n` has no stiffness in from its members,
    /// supports and springs, when no frame member meets it (none: a frame
    /// meets it, or its bars, supports and springs reach every
    /// direction): unit vectors, structure axes.
    pub fn bar_joint_free(&self, n: usize) -> Vec<[f64; 3]> {
        if self.frame_at(n) {
            return Vec::new();
        }
        crate::stabilise::free_translations(self, n)
    }

    /// The error of a solve whose loads `nodal` push a node carrying an
    /// attachment point across its bars (only trusses, cables or
    /// tension-only members meet it, all in a line or a plane — a joint
    /// inserted on a bar): such a joint is a mechanism across them, and
    /// holding it (stabilisation) would only share the load to the bars'
    /// ends. None: no such load.
    pub fn bar_joint_error(&self, nodal: &[NodalLoad]) -> Option<String> {
        let mut done: Vec<usize> = Vec::new();
        for p in &self.joint_points {
            if done.contains(&p.node) {
                continue;
            }
            done.push(p.node);
            let free = self.bar_joint_free(p.node);
            if free.is_empty() {
                continue;
            }
            let f = nodal
                .iter()
                .filter(|l| l.node == p.node)
                .fold([0.0; 3], |a, l| {
                    [a[0] + l.force[0], a[1] + l.force[1], a[2] + l.force[2]]
                });
            let size = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
            let across = free
                .iter()
                .map(|d| (d[0] * f[0] + d[1] * f[1] + d[2] * f[2]).abs())
                .fold(0.0, f64::max);
            if across > (ACROSS * size).max(ACROSS_N) {
                let points: Vec<&str> = self
                    .joint_points
                    .iter()
                    .filter(|q| q.node == p.node)
                    .map(|q| q.label.as_str())
                    .collect();
                return Some(format!(
                    "joint {} (attachment point {}): only bars meet it, in a line or a plane — \
                     the load across them ({:.3} kN) cannot be carried there; make the members \
                     frames, or add a member to the joint",
                    self.nodes[p.node].id,
                    points.join(", "),
                    across / 1000.0
                ));
            }
        }
        None
    }

    /// A node's id.
    pub fn node_name(&self, index: usize) -> String {
        self.nodes
            .get(index)
            .map_or_else(|| format!("#{index}"), |n| n.id.clone())
    }
}
