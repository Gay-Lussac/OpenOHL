// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Attachment points linked to joints (2026-10-21): where a point's load
//! acts in the model, the moment of its offset (a rigid link), and the
//! split of a frame member at a point along it.
//!
//! - A point on a node: its force at the node; with an offset r (from the
//!   joint to the point) also the moment r × F where a frame member meets
//!   the node. Where only trusses (cables, tension-only members) meet it
//!   the moment cannot be carried: it is left out and said so
//!   ([`PointLoad::note`]) — make the joint a frame joint to carry it.
//! - A point along a frame member strictly between its ends: the analysis
//!   splits the member there ([`Model::split`]) — a node at the point,
//!   the member two (or more) pieces of the same properties, each
//!   buckling over the member's whole effective length — and the force
//!   (and the moment) act at that node; results are reported per member
//!   ([`crate::solve::Solution::pieces`]). Along a truss member the force
//!   is shared to its ends (1 − at, at), statically equivalent for the
//!   member's axis; a truss carries no moment there.

use ohl_core::units::Length;
use serde::Deserialize;

use crate::model::{JointPoint, Member, MemberKind, MemberPoint, Model, NodalLoad, Node, cross};

/// Where an attachment point's load goes in the model.
#[derive(Debug, Clone, PartialEq)]
pub struct PointLoad {
    /// The loads at the nodes (a split member's node included:
    /// [`Model::split`]).
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

/// Where in the model a point sits.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Place {
    /// On a node.
    Node(usize),
    /// Along a member: the member point (index into
    /// [`Model::member_points`]).
    Along(usize),
}

/// A node the analysis adds where attachment points sit along a frame
/// member ([`Model::split`]).
#[derive(Debug, Clone, PartialEq)]
pub struct SplitNode {
    /// Its id: the joint the points there are linked to.
    pub id: String,
    /// The member split (index).
    pub member: usize,
    /// Fraction of the member's length from its first node.
    pub at: f64,
    /// Where it is, structure axes, m.
    pub point: [f64; 3],
}

/// A model with its frame members split at the attachment points along
/// them ([`Model::split`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Split {
    /// The model analysed: the nodes, then the split nodes
    /// ([`Model::split_nodes`], in order); the members in their places
    /// (a split one becomes its first piece), then the further pieces.
    pub model: Model,
    /// Every piece of a split member: (member of the original model,
    /// member of [`Split::model`], from, to: fractions along the
    /// original).
    pub pieces: Vec<(usize, usize, f64, f64)>,
}

fn is_zero(v: [f64; 3]) -> bool {
    v == [0.0; 3]
}

fn kn_m(m: [f64; 3]) -> f64 {
    (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt() / 1000.0
}

impl Model {
    /// The points of a structure file's `[[point]]` tables linked to the
    /// model's nodes and members, by the structure's rule
    /// ([`ohl_structures::point_joints`]): a point on a node (or at a
    /// node's position, or named by a node's `attachment`) is a
    /// [`JointPoint`], one along a member a [`MemberPoint`]; a point at a
    /// position of its own on no node has no place in the model (its
    /// load is refused: [`Model::point_load`] gives none).
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
            let node = p.get("node").and_then(toml::Value::as_str);
            let member = p.get("member").and_then(toml::Value::as_str);
            let at = p
                .get("at")
                .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)));
            let place = match (node, member, at) {
                (Some(n), _, _) => ohl_structures::PointPlace::Joint(n),
                (None, Some(m), Some(at)) => ohl_structures::PointPlace::Member(m, at),
                (None, Some(m), None) => {
                    return Err(format!("point {label}: member {m} without at"));
                }
                _ => match p.get("point") {
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
        let mut member_points = Vec::new();
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
                ohl_structures::PointPlace::Member(m, at) => {
                    let member = self
                        .members
                        .iter()
                        .position(|x| x.id == m)
                        .ok_or_else(|| format!("point {label}: no member {m}"))?;
                    member_points.push(MemberPoint {
                        label: (*label).to_owned(),
                        member,
                        at,
                        joint,
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
        self.member_points = member_points;
        Ok(())
    }

    /// The node carrying the structure's point labelled `label`: a point on
    /// a node, or a separate model's node naming it ([`Node::attachment`]).
    pub fn attachment_node(&self, label: &str) -> Option<usize> {
        self.joint_points
            .iter()
            .find(|p| p.label == label)
            .map(|p| p.node)
            .or_else(|| {
                self.nodes
                    .iter()
                    .position(|n| n.attachment.as_deref() == Some(label))
            })
    }

    /// Where the point labelled `label` sits, and its offset.
    fn place_of(&self, label: &str) -> Option<(Place, [f64; 3])> {
        if let Some(p) = self.joint_points.iter().find(|p| p.label == label) {
            return Some((Place::Node(p.node), p.offset));
        }
        if let Some(k) = self.member_points.iter().position(|p| p.label == label) {
            return Some((Place::Along(k), self.member_points[k].offset));
        }
        self.nodes
            .iter()
            .position(|n| n.attachment.as_deref() == Some(label))
            .map(|n| (Place::Node(n), [0.0; 3]))
    }

    /// The load of the point labelled `label` with `force` (N, structure
    /// axes), linked as the structure file (or the separate model's
    /// node) says. None: no such point in the model.
    pub fn point_load(&self, label: &str, force: [f64; 3]) -> Option<PointLoad> {
        let (place, offset) = self.place_of(label)?;
        Some(self.load_at(label, place, offset, force))
    }

    /// The load of point `label` linked to joint `joint` at `offset`
    /// (structure axes, m) with `force` (N): the link a placed structure
    /// gives (the line's attachment point: its own relink wins over the
    /// type's). The joint is a node of the model, or a joint the points
    /// make along a member; a separate model (`.ohlfe`, its own node ids)
    /// is looked up by the point's label, the offset kept. None: neither.
    pub fn linked_load(
        &self,
        label: &str,
        joint: &str,
        offset: [f64; 3],
        force: [f64; 3],
    ) -> Option<PointLoad> {
        let place = self
            .nodes
            .iter()
            .position(|n| n.id == joint)
            .map(Place::Node)
            .or_else(|| {
                self.member_points
                    .iter()
                    .position(|p| p.joint == joint)
                    .map(Place::Along)
            })
            .or_else(|| self.place_of(label).map(|(p, _)| p))?;
        Some(self.load_at(label, place, offset, force))
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

    fn load_at(&self, label: &str, place: Place, offset: [f64; 3], force: [f64; 3]) -> PointLoad {
        let moment = if is_zero(offset) {
            [0.0; 3]
        } else {
            cross(offset, force)
        };
        let some = !is_zero(moment);
        let at = |node: usize, share: f64, m: bool| NodalLoad {
            node,
            force: force.map(|f| f * share),
            moment: if m {
                moment.map(|x| x * share)
            } else {
                [0.0; 3]
            },
        };
        let left = |at: &str| {
            format!(
                "attachment point {label}: the moment of its offset ({:.2} kN m) left out at \
                 {at}: only trusses meet it (make it a frame joint to carry it)",
                kn_m(moment)
            )
        };
        match place {
            Place::Node(n) => {
                let frame = self.frame_at(n);
                PointLoad {
                    loads: vec![at(n, 1.0, frame && some)],
                    moment,
                    moment_applied: frame && some,
                    note: (some && !frame).then(|| left(&format!("joint {}", self.nodes[n].id))),
                }
            }
            Place::Along(k) => {
                let p = &self.member_points[k];
                if let Some(node) = self.split_index(p) {
                    return PointLoad {
                        loads: vec![at(node, 1.0, some)],
                        moment,
                        moment_applied: some,
                        note: None,
                    };
                }
                let m = &self.members[p.member];
                let ends = [(m.i, 1.0 - p.at), (m.j, p.at)];
                let carried = ends
                    .iter()
                    .all(|&(n, share)| share == 0.0 || self.frame_at(n));
                PointLoad {
                    loads: ends
                        .iter()
                        .map(|&(n, share)| at(n, share, some && carried))
                        .collect(),
                    moment,
                    moment_applied: some && carried,
                    note: (some && !carried).then(|| left(&format!("member {}", m.id))),
                }
            }
        }
    }

    /// Whether the analysis splits member point `p`'s member there: a
    /// frame member, strictly between its ends.
    fn splits(&self, p: &MemberPoint) -> bool {
        self.members[p.member].kind == MemberKind::Frame && p.at > 0.0 && p.at < 1.0
    }

    /// The nodes the analysis adds along frame members at the attachment
    /// points there ([`Model::split`]), one per place, in the order of
    /// the points.
    pub fn split_nodes(&self) -> Vec<SplitNode> {
        let mut out: Vec<SplitNode> = Vec::new();
        for p in self.member_points.iter().filter(|p| self.splits(p)) {
            if out
                .iter()
                .any(|s| s.member == p.member && s.at.to_bits() == p.at.to_bits())
            {
                continue;
            }
            let m = &self.members[p.member];
            let (a, b) = (self.nodes[m.i].point, self.nodes[m.j].point);
            out.push(SplitNode {
                id: p.joint.clone(),
                member: p.member,
                at: p.at,
                point: [0, 1, 2].map(|k| a[k] + p.at * (b[k] - a[k])),
            });
        }
        out
    }

    /// The node of the analysed model ([`Model::split`]) where member
    /// point `p` acts, when its member is split there.
    fn split_index(&self, p: &MemberPoint) -> Option<usize> {
        if !self.splits(p) {
            return None;
        }
        self.split_nodes()
            .iter()
            .position(|s| s.member == p.member && s.at.to_bits() == p.at.to_bits())
            .map(|k| self.nodes.len() + k)
    }

    /// A node's id, the split nodes' included ([`Model::split_nodes`]).
    pub fn node_name(&self, index: usize) -> String {
        match self.nodes.get(index) {
            Some(n) => n.id.clone(),
            None => self
                .split_nodes()
                .get(index - self.nodes.len())
                .map_or_else(|| format!("#{index}"), |s| s.id.clone()),
        }
    }

    /// The model the analysis solves when attachment points sit along
    /// frame members (none: no such point, the model as it is): a node at
    /// each such place ([`Model::split_nodes`]), the member split there
    /// into pieces of its properties — its first piece in its place, the
    /// others after the members —, each piece buckling over the whole
    /// member's effective length (its `lk`, else k × its length), so the
    /// split changes no capacity.
    pub fn split(&self) -> Option<Split> {
        let splits = self.split_nodes();
        if splits.is_empty() {
            return None;
        }
        let mut model = self.clone();
        let base = self.nodes.len();
        for s in &splits {
            model.nodes.push(Node::free(&s.id, s.point));
        }
        let mut pieces = Vec::new();
        let mut members: Vec<usize> = splits.iter().map(|s| s.member).collect();
        members.sort_unstable();
        members.dedup();
        for m in members {
            let original = &self.members[m];
            let lk = original.lk.or(Some(original.k * self.length(original)));
            let mut at: Vec<(f64, usize)> = splits
                .iter()
                .enumerate()
                .filter(|(_, s)| s.member == m)
                .map(|(k, s)| (s.at, base + k))
                .collect();
            at.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut ends: Vec<(f64, usize)> = vec![(0.0, original.i)];
            ends.extend(at);
            ends.push((1.0, original.j));
            for (k, w) in ends.windows(2).enumerate() {
                let piece = Member {
                    i: w[0].1,
                    j: w[1].1,
                    lk,
                    ..original.clone()
                };
                let index = if k == 0 {
                    model.members[m] = piece;
                    m
                } else {
                    model.members.push(piece);
                    model.members.len() - 1
                };
                pieces.push((m, index, w[0].0, w[1].0));
            }
        }
        model.member_points.clear();
        model.joint_points.clear();
        Some(Split { model, pieces })
    }
}
