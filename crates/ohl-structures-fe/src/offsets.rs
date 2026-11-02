// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Eccentric member ends (owner 2026-11-02, stage 4.4 of the [joint
//! details]; built, in active development, not approved yet): a member
//! end moved sideways so it fits on the gusset plate or beside another
//! member (a connection's `shift`) has its axis beside the joint's work
//! point, its force passing at the eccentricity e = shift.
//!
//! - **The offset** ([`from_shifts`]): r = shift · u, u the shift's
//!   direction ([`ohl_structures::connections::shift_direction`]) — across
//!   the member, towards the member it bolts onto (given), else towards
//!   the joint's leg ([`crate::member_ends::joint_leg`], a plate's too).
//!   It is the member's rigid end offset ([`crate::Member::offsets`]).
//! - **In the solve** ([`crate::solve()`]): the member's end at joint + r,
//!   tied rigidly to the joint: u_end = u_joint + θ_joint × r, θ_end =
//!   θ_joint. A frame member takes the standard rigid end offset
//!   transformation of its stiffness and end forces (its length and axes
//!   between its offset ends): it gets the moments of the offset. A truss
//!   (cable, tension-only member) has its axial force along the line
//!   between its offset ends; its joint receives the force and the
//!   moment r × F.
//! - **Only where a frame member meets the joint** (the joint has
//!   rotations): where only bars meet it, the moment cannot be carried —
//!   that end's offset is **left out** (the force passes through the work
//!   point, as without a shift) and said, per member and per case
//!   ([`EccentricEnd::carried`], [`eccentricity_note`]): on the unsafe
//!   side for the joint's members, never a singular stiffness. The same
//!   policy as an attachment point's offset ([`crate::links`]).
//! - The rigid link's own second-order terms (the force turning with the
//!   link as the joint rotates) are left out of P-Delta: the member's
//!   chord is taken between its offset ends.
//!
//! A tower without shifts has no offsets and solves exactly as before.
//!
//! [joint details]: https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/joint-details.md

use crate::design::Onto;
use crate::element::Mat12;
use crate::member_ends::{arriving, joint_leg};
use crate::model::{Model, cross, norm};
use crate::solve::Solution;

/// The direction (unit, from the joint) a shift at end `end` of member
/// `idx` is measured towards, and the member it is: the member it bolts
/// onto (given), else the joint's leg (a plate's, or nothing given).
pub fn shift_towards(model: &Model, idx: usize, end: usize) -> Option<(usize, [f64; 3])> {
    let m = &model.members[idx];
    let node = if end == 0 { m.i } else { m.j };
    let all = arriving(model, node);
    let given = m.design.ends[end].as_ref().and_then(|e| e.onto.as_ref());
    if let Some(Onto::Member(k)) = given {
        return all
            .iter()
            .find(|(x, _)| x == k)
            .map(|(x, a)| (*x, a.direction));
    }
    let own = all.iter().position(|(k, _)| *k == idx)?;
    let list: Vec<_> = all.iter().map(|(_, a)| *a).collect();
    joint_leg(own, &list).map(|j| (all[j].0, all[j].1.direction))
}

/// The members' rigid end offsets ([`crate::Member::offsets`]) from their
/// connections' shifts ([`crate::design::EndDesign::shift`]); a shift
/// without a direction (nothing to measure it towards, or that runs in
/// line with the member) is left out and said in the end's notes.
pub(crate) fn from_shifts(model: &mut Model) {
    for idx in 0..model.members.len() {
        for end in 0..2 {
            let Some(shift) = model.members[idx].design.ends[end]
                .as_ref()
                .and_then(|e| e.shift)
            else {
                continue;
            };
            let m = &model.members[idx];
            let (a, b) = if end == 0 { (m.i, m.j) } else { (m.j, m.i) };
            let d = crate::model::sub(model.nodes[b].point, model.nodes[a].point);
            let l = norm(d);
            let along = d.map(|c| c / l);
            let towards = shift_towards(model, idx, end);
            let u =
                towards.and_then(|(_, t)| ohl_structures::connections::shift_direction(along, t));
            let note = match (towards, u) {
                (_, Some(u)) => {
                    model.members[idx].offsets[end] = u.map(|c| c * shift);
                    continue;
                }
                (None, _) => format!(
                    "shift {:.0} mm left out: nothing at the joint to measure it towards",
                    shift * 1e3
                ),
                (Some((k, _)), None) => format!(
                    "shift {:.0} mm left out: {} runs in line with it",
                    shift * 1e3,
                    model.members[k].id
                ),
            };
            if let Some(e) = model.members[idx].design.ends[end].as_mut() {
                e.notes.push(note);
            }
        }
    }
}

/// One eccentric member end ([`Model::eccentric_ends`]).
#[derive(Debug, Clone, PartialEq)]
pub struct EccentricEnd {
    /// The end: 0 at the member's first joint, 1 at its second.
    pub end: usize,
    /// Its joint (node index).
    pub node: usize,
    /// The rigid end offset, structure axes, m.
    pub offset: [f64; 3],
    /// The eccentricity, m (the offset's length).
    pub e: f64,
    /// A frame member meets the joint: the offset is in the solve, its
    /// moment carried; false: left out, only bars meet the joint.
    pub carried: bool,
}

impl Model {
    /// The ends of member `idx` with a rigid end offset, and whether each
    /// is carried (a frame member meets its joint: [`Model::frame_at`]).
    pub fn eccentric_ends(&self, idx: usize) -> Vec<EccentricEnd> {
        let m = &self.members[idx];
        (0..2)
            .filter(|&end| m.offsets[end] != [0.0; 3])
            .map(|end| {
                let node = if end == 0 { m.i } else { m.j };
                EccentricEnd {
                    end,
                    node,
                    offset: m.offsets[end],
                    e: norm(m.offsets[end]),
                    carried: self.frame_at(node),
                }
            })
            .collect()
    }

    /// Whether any member has an eccentric end.
    pub fn has_offsets(&self) -> bool {
        self.members.iter().any(|m| m.offsets != [[0.0; 3]; 2])
    }
}

/// The moment of an end's offset under axial force `n` (N), N·m: r × F,
/// F the force on the joint along the member's chord between its offset
/// ends.
pub fn end_moment(model: &Model, idx: usize, end: &EccentricEnd, n: f64) -> [f64; 3] {
    let m = &model.members[idx];
    let d = crate::model::sub(model.end_point(m, 1), model.end_point(m, 0));
    let l = norm(d);
    // On the first joint the member pulls towards its second (tension).
    let sign = if end.end == 0 { 1.0 } else { -1.0 };
    let f = d.map(|c| sign * n * c / l);
    cross(end.offset, f)
}

/// The eccentric ends of member `idx` in words, under its axial force `n`
/// (N), for what governs (none: no eccentric end): per end its joint, e,
/// and where the moment went — `N2 e 15 mm, 1.20 kN m to the joint's
/// frames`, `N2 e 15 mm, in its own end moments`, `N3 e 15 mm, 0.80 kN m
/// LEFT OUT (only bars meet N3)`.
pub fn eccentricity_words(model: &Model, idx: usize, n: f64) -> Option<String> {
    let ends = model.eccentric_ends(idx);
    if ends.is_empty() {
        return None;
    }
    let m = &model.members[idx];
    let s = &model.sections[m.section];
    let axial_check =
        !m.is_frame() || (crate::check::axial_member(m, s) && !model.analysis.single_angle_moments);
    let parts: Vec<String> = ends
        .iter()
        .map(|e| {
            let joint = model.node_name(e.node);
            let size = norm(end_moment(model, idx, e, n)) / 1000.0;
            let head = format!("{joint} e {:.0} mm", e.e * 1e3);
            if !e.carried {
                format!(
                    "{head}, {size:.2} kN m LEFT OUT (only bars meet {joint}: the force through \
                     the work point)"
                )
            } else if m.is_frame() {
                if axial_check {
                    format!(
                        "{head}, {size:.2} kN m in its end moments and the joint's frames (its \
                         check axial only)"
                    )
                } else {
                    format!("{head}, {size:.2} kN m in its end moments and the joint's frames")
                }
            } else {
                format!("{head}, {size:.2} kN m to the joint's frame members")
            }
        })
        .collect();
    Some(parts.join("; "))
}

/// The eccentric member ends of a solution in words (none: the model has
/// none): how many ends of how many members, and those whose moment was
/// left out (only bars meet their joint) with the moment of this case.
pub fn eccentricity_note(model: &Model, solution: &Solution) -> Option<String> {
    if !model.has_offsets() {
        return None;
    }
    let mut ends = 0;
    let mut members = 0;
    let mut left_out = Vec::new();
    for idx in 0..model.members.len() {
        let list = model.eccentric_ends(idx);
        if list.is_empty() {
            continue;
        }
        members += 1;
        ends += list.len();
        let n = solution.members.get(idx).map_or(0.0, |f| f.axial);
        for e in list.iter().filter(|e| !e.carried) {
            left_out.push(format!(
                "{} at {} {:.2} kN m",
                model.members[idx].id,
                model.node_name(e.node),
                norm(end_moment(model, idx, e, n)) / 1000.0
            ));
        }
    }
    let mut text = format!(
        "eccentric member ends (connection shifts): {ends} end(s) of {members} member(s), \
         rigid end offsets to their joints"
    );
    if !left_out.is_empty() {
        let shown = 12;
        text.push_str(&format!(
            "; LEFT OUT at {} end(s) where only bars meet the joint (the moment cannot be \
             carried; unsafe side — make it a frame joint): {}{}",
            left_out.len(),
            left_out
                .iter()
                .take(shown)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            if left_out.len() > shown { ", …" } else { "" }
        ));
    }
    Some(text)
}

/// The rigid end offset transformation (structure axes): the 12
/// displacements of a member's two ends from those of its joints, u_end =
/// u_joint + θ × r = u_joint − [r]× θ, θ_end = θ_joint. Its transpose
/// takes the end forces to the joints: F, and M + r × F.
pub(crate) fn rigid(offsets: [[f64; 3]; 2]) -> Mat12 {
    let mut d = Mat12::identity();
    for (b, r) in offsets.iter().enumerate() {
        let o = 6 * b;
        // −[r]×: row k gives the k-th component of θ × r.
        d[(o, o + 4)] = r[2];
        d[(o, o + 5)] = -r[1];
        d[(o + 1, o + 3)] = -r[2];
        d[(o + 1, o + 5)] = r[0];
        d[(o + 2, o + 3)] = r[1];
        d[(o + 2, o + 4)] = -r[0];
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::SMatrix;

    /// The rigid transformation gives θ × r: a rotation about z of a
    /// joint moves an end offset along x sideways along y.
    #[test]
    fn rigid_gives_theta_cross_r() {
        let d = rigid([[0.1, 0.0, 0.0], [0.0, 0.0, 0.0]]);
        let mut g = SMatrix::<f64, 12, 1>::zeros();
        g[5] = 0.02;
        let e = d * g;
        assert!((e[1] - 0.02 * 0.1).abs() < 1e-15);
        assert!(e[0].abs() < 1e-15 && e[2].abs() < 1e-15);
        // Its transpose: a force along y at the end gives the joint the
        // moment r × F about z.
        let mut f = SMatrix::<f64, 12, 1>::zeros();
        f[1] = 1000.0;
        let j = d.transpose() * f;
        assert!((j[5] - 100.0).abs() < 1e-12);
        assert!((j[1] - 1000.0).abs() < 1e-12);
    }
}
