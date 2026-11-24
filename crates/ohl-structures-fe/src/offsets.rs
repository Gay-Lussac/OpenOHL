// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Eccentric member ends (owner 2026-11-02, stage 4.4 of the [joint
//! details]; built, in active development, not approved yet): a member
//! end moved sideways so it fits on the gusset plate or beside another
//! member (a connection's `shift`) has its axis beside the joint's work
//! point, its force passing at the eccentricity e = shift.
//!
//! - **The offset** (`from_shifts`): r = shift · u, u the shift's
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
//!   rotations): where only bars meet it, the moment cannot be carried in
//!   the solve — that end's offset is left out of it (the force passes
//!   through the work point, as without a shift), never a singular
//!   stiffness. Since 2026-11-18 (the lattice towers' safe-side items)
//!   the moment is **carried into the member's own check**
//!   ([`with_eccentric_moments`]): the shifted member takes the whole
//!   moment N·e of that end as a bending moment, uniform along it (on the
//!   safe side for it: the joint's other members, which may take part of
//!   it, are not relieved of anything — a statically admissible share,
//!   the joint in balance with the shifted member holding it all), its
//!   code's interaction of axial force and bending checking it. A cable
//!   cannot bend: its moment stays left out, said NOT ON THE SAFE SIDE.
//!   Said per member and per case ([`EccentricEnd::carried`],
//!   [`eccentricity_note`]).
//! - **An offset whose sign is not known** ([`crate::Member::free_offsets`];
//!   2026-11-19, the owner's decision on O45 A1: an estimated
//!   eccentricity's upper bound, a plate's side) is never in the solve, at
//!   a frame joint either: the member's own check takes its moment N·e
//!   whole, uniform along it, as where only bars meet a joint, and
//!   **sign-free** — about each of the member's axes the sizes of the
//!   moments of its parts added (the worst of their signs), with the sign
//!   that adds to the moment the solve gave. On the safe side for the
//!   member (it holds the whole moment); statically admissible for the
//!   others (the joint in balance with the member holding it all: nothing
//!   relieved, nothing added to them). Two solves instead of one per sign,
//!   no mix of signs left uncovered, and no joint turned by an offset of
//!   unknown sign in the solve.
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
    /// Its offsets whose sign is not known ([`crate::Member::free_offsets`]):
    /// never in the solve, their moment in its own check, sign-free
    /// (2026-11-19).
    pub free: Vec<[f64; 3]>,
}

impl EccentricEnd {
    /// The eccentricity of its offsets whose sign is not known, m: the
    /// length of their parts taken square to each other (their sizes'
    /// root sum of squares).
    pub fn free_e(&self) -> f64 {
        self.free
            .iter()
            .map(|r| norm(*r).powi(2))
            .sum::<f64>()
            .sqrt()
    }
}

impl Model {
    /// The ends of member `idx` with a rigid end offset, and whether each
    /// is carried (a frame member meets its joint: [`Model::frame_at`]).
    pub fn eccentric_ends(&self, idx: usize) -> Vec<EccentricEnd> {
        let m = &self.members[idx];
        (0..2)
            .filter(|&end| m.offsets[end] != [0.0; 3] || free_of(m, end).next().is_some())
            .map(|end| {
                let node = if end == 0 { m.i } else { m.j };
                EccentricEnd {
                    end,
                    node,
                    offset: m.offsets[end],
                    e: norm(m.offsets[end]),
                    carried: self.frame_at(node),
                    free: free_of(m, end).collect(),
                }
            })
            .collect()
    }

    /// Whether any member has an eccentric end (an offset of either kind).
    pub fn has_offsets(&self) -> bool {
        self.members
            .iter()
            .any(|m| m.offsets != [[0.0; 3]; 2] || (0..2).any(|e| free_of(m, e).next().is_some()))
    }
}

/// Member `m`'s offsets of unknown sign at end `end` that are not zero.
fn free_of(m: &crate::Member, end: usize) -> impl Iterator<Item = [f64; 3]> + '_ {
    m.free_offsets[end]
        .iter()
        .copied()
        .filter(|r| *r != [0.0; 3])
}

impl EccentricEnd {
    /// The offsets its member's own check takes (not in the solve): the
    /// offset where only bars meet the joint, and every offset of unknown
    /// sign.
    fn uncarried_parts(&self) -> Vec<[f64; 3]> {
        let mut out: Vec<[f64; 3]> = Vec::new();
        if !self.carried && self.offset != [0.0; 3] {
            out.push(self.offset);
        }
        out.extend(self.free.iter().copied());
        out
    }
}

/// The moment of an end's offset under axial force `n` (N), N·m: r × F,
/// F the force on the joint along the member's chord between its offset
/// ends.
pub fn end_moment(model: &Model, idx: usize, end: &EccentricEnd, n: f64) -> [f64; 3] {
    offset_moment(model, idx, end.end, end.offset, n)
}

/// The moment of offset `r` at end `end` of member `idx` under axial force
/// `n` (N), N·m: r × F as [`end_moment`].
fn offset_moment(model: &Model, idx: usize, end: usize, r: [f64; 3], n: f64) -> [f64; 3] {
    let m = &model.members[idx];
    let d = crate::model::sub(model.end_point(m, 1), model.end_point(m, 0));
    let l = norm(d);
    // On the first joint the member pulls towards its second (tension).
    let sign = if end == 0 { 1.0 } else { -1.0 };
    let f = d.map(|c| sign * n * c / l);
    cross(r, f)
}

/// The size of the moment of the offsets of unknown sign of `end`
/// (member `idx`, axial force `n`), N·m: their parts' moments taken square
/// to each other (the parts lie square to each other: a shift across the
/// member and a plate's offset out of its face).
pub fn free_moment(model: &Model, idx: usize, end: &EccentricEnd, n: f64) -> f64 {
    end.free
        .iter()
        .map(|r| norm(offset_moment(model, idx, end.end, *r, n)).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// The moments of member `idx`'s eccentric ends its solve did not carry
/// — an offset where only bars meet the joint ([`EccentricEnd::carried`]
/// false; 2026-11-18) and every offset whose sign is not known
/// ([`EccentricEnd::free`]; 2026-11-19, at frame joints too) — about its
/// local y and z, N·m (sizes): per end r × F under its axial force `n`,
/// about each axis the sizes of its parts' moments added (the worst of the
/// unknown signs: sign-free), each axis the larger of its two ends'. None:
/// no such end, or the member is a cable (it cannot bend).
pub fn uncarried_moments(model: &Model, idx: usize, n: f64) -> Option<[f64; 2]> {
    let m = &model.members[idx];
    if m.kind == crate::MemberKind::Cable {
        return None;
    }
    let ends: Vec<Vec<[f64; 3]>> = model
        .eccentric_ends(idx)
        .iter()
        .map(|e| {
            e.uncarried_parts()
                .into_iter()
                .map(|r| offset_moment(model, idx, e.end, r, n))
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
        .collect();
    if ends.is_empty() {
        return None;
    }
    let [_, y, z] = member_axes(model, idx);
    let mut out = [0.0f64; 2];
    for parts in &ends {
        let (mut my, mut mz) = (0.0f64, 0.0f64);
        for mo in parts {
            my += (mo[0] * y[0] + mo[1] * y[1] + mo[2] * y[2]).abs();
            mz += (mo[0] * z[0] + mo[1] * z[1] + mo[2] * z[2]).abs();
        }
        out[0] = out[0].max(my);
        out[1] = out[1].max(mz);
    }
    Some(out)
}

/// Member `idx`'s local axes as its solve takes them (rows x, y, z in
/// structure axes): between its ends moved by the offsets the solve
/// carries (a frame member meets the joint), else between its joints.
pub fn member_axes(model: &Model, idx: usize) -> [[f64; 3]; 3] {
    let m = &model.members[idx];
    let p = |end: usize| {
        let node = if end == 0 { m.i } else { m.j };
        let q = model.nodes[node].point;
        if model.frame_at(node) {
            let r = m.offsets[end];
            [q[0] + r[0], q[1] + r[1], q[2] + r[2]]
        } else {
            q
        }
    };
    let r = crate::element::axes_between(p(0), p(1), m);
    [0, 1, 2].map(|k| [r[(k, 0)], r[(k, 1)], r[(k, 2)]])
}

/// Member `idx`'s end forces `f` (on the member, its local axes) in
/// structure axes, per end `[F_x, F_y, F_z, M_x, M_y, M_z]` (N, N·m;
/// 2026-11-18: the plate checks take a frame member's end shear and
/// moments).
pub fn global_end_forces(
    model: &Model,
    idx: usize,
    f: &crate::solve::MemberForces,
) -> [[f64; 6]; 2] {
    let ax = member_axes(model, idx);
    let mut out = [[0.0; 6]; 2];
    for (e, o) in out.iter_mut().enumerate() {
        for part in 0..2 {
            for (k, row) in ax.iter().enumerate() {
                let v = f.end_forces[6 * e + 3 * part + k];
                for (c, r) in row.iter().enumerate() {
                    o[3 * part + c] += v * r;
                }
            }
        }
    }
    out
}

/// Member `idx`'s forces `f` as its check takes them (2026-11-18): with
/// the moments of its eccentric ends the solve could not carry
/// ([`uncarried_moments`]) added as a **uniform** bending moment along it
/// — the larger end's about each axis over its whole length (on the safe
/// side: the largest moment everywhere, the most onerous shape for its
/// buckling), with the sign that adds to the moment its solve gave
/// (internal moment `M_i = −F[4]`, `M_j = F[10]` about y; 5, 11 about z).
/// A member without such an end: `f` as it is.
pub fn with_eccentric_moments(
    model: &Model,
    idx: usize,
    f: &crate::solve::MemberForces,
) -> crate::solve::MemberForces {
    let Some(add) = uncarried_moments(model, idx, f.axial) else {
        return *f;
    };
    let mut out = *f;
    for (k, (i, j)) in [(4usize, 10usize), (5, 11)].into_iter().enumerate() {
        if add[k] == 0.0 {
            continue;
        }
        let (mi, mj) = (-out.end_forces[i], out.end_forces[j]);
        let larger = if mi.abs() >= mj.abs() { mi } else { mj };
        let sign = if larger < 0.0 { -1.0 } else { 1.0 };
        out.end_forces[i] -= sign * add[k];
        out.end_forces[j] += sign * add[k];
    }
    out
}

/// Whether member `idx` is checked with the moments of eccentric ends its
/// solve could not carry ([`uncarried_moments`]): its code's check then
/// takes its bending with its axial force, even a truss's.
pub fn eccentric_bending(model: &Model, idx: usize) -> bool {
    model.members[idx].kind != crate::MemberKind::Cable
        && model
            .eccentric_ends(idx)
            .iter()
            .any(|e| !e.uncarried_parts().is_empty())
}

/// The eccentric ends of member `idx` in words, under its axial force `n`
/// (N), for what governs (none: no eccentric end): per end its joint, e,
/// and where the moment went — `N2 e 15 mm, 1.20 kN m to the joint's
/// frames`, `N2 e 15 mm, in its own end moments`, `N3 e 15 mm, 0.80 kN m
/// in its own check (only bars meet N3 …)`; a cable's `LEFT OUT`.
pub fn eccentricity_words(model: &Model, idx: usize, n: f64) -> Option<String> {
    let ends = model.eccentric_ends(idx);
    if ends.is_empty() {
        return None;
    }
    let m = &model.members[idx];
    let parts: Vec<String> = ends
        .iter()
        .flat_map(|e| {
            let joint = model.node_name(e.node);
            let free = (!e.free.is_empty()).then(|| {
                let size = free_moment(model, idx, e, n) / 1000.0;
                let head = format!("{joint} e {:.0} mm, its sign not known", e.free_e() * 1e3);
                if m.kind == crate::MemberKind::Cable {
                    format!(
                        "{head}, {size:.2} kN m LEFT OUT (a cable cannot bend — NOT ON THE SAFE \
                         SIDE: the moment is missing from the members there, so their usage can \
                         be too low; give the offset's side, or make it a frame joint)"
                    )
                } else {
                    format!(
                        "{head}, {size:.2} kN m in its own check, whole and sign-free, uniform \
                         along it (never in the solve)"
                    )
                }
            });
            let signed = (e.offset != [0.0; 3]).then(|| signed_words(model, idx, e, n));
            signed.into_iter().chain(free)
        })
        .collect();
    Some(parts.join("; "))
}

/// The words of end `e`'s offset of known sign (member `idx`, axial force
/// `n`): where its moment went.
fn signed_words(model: &Model, idx: usize, e: &EccentricEnd, n: f64) -> String {
    let m = &model.members[idx];
    let s = &model.sections[m.section];
    let axial_check =
        !m.is_frame() || (crate::check::axial_member(m, s) && !model.analysis.single_angle_moments);
    let joint = model.node_name(e.node);
    let size = norm(end_moment(model, idx, e, n)) / 1000.0;
    let head = format!("{joint} e {:.0} mm", e.e * 1e3);
    if !e.carried && m.kind == crate::MemberKind::Cable {
        format!(
            "{head}, {size:.2} kN m LEFT OUT (only bars meet {joint}: the force through the work \
             point; a cable cannot bend — NOT ON THE SAFE SIDE: the moment is missing from the \
             members there, so their usage can be too low; make it a frame joint)"
        )
    } else if !e.carried {
        format!(
            "{head}, {size:.2} kN m in its own check, uniform along it (only bars meet {joint}: \
             the force through the work point in the solve, the member takes the moment whole)"
        )
    } else if m.is_frame() {
        if axial_check {
            format!(
                "{head}, {size:.2} kN m in its end moments and the joint's frames (its check axial \
                 only)"
            )
        } else {
            format!("{head}, {size:.2} kN m in its end moments and the joint's frames")
        }
    } else {
        format!("{head}, {size:.2} kN m to the joint's frame members")
    }
}

/// The eccentric member ends of a solution in words (none: the model has
/// none): how many ends of how many members, those whose moment the solve
/// could not carry (only bars meet their joint) — taken by the member's
/// own check — with the moment of this case, and a cable's left out.
pub fn eccentricity_note(model: &Model, solution: &Solution) -> Option<String> {
    if !model.has_offsets() {
        return None;
    }
    let mut ends = 0;
    let mut members = 0;
    let mut own = Vec::new();
    let mut left_out = Vec::new();
    let mut free = Vec::new();
    let mut free_left = Vec::new();
    for idx in 0..model.members.len() {
        let list = model.eccentric_ends(idx);
        if list.is_empty() {
            continue;
        }
        members += 1;
        ends += list.len();
        let n = solution.members.get(idx).map_or(0.0, |f| f.axial);
        let cable = model.members[idx].kind == crate::MemberKind::Cable;
        for e in list.iter().filter(|e| !e.free.is_empty()) {
            let words = format!(
                "{} at {} {:.2} kN m",
                model.members[idx].id,
                model.node_name(e.node),
                free_moment(model, idx, e, n) / 1000.0
            );
            if cable {
                free_left.push(words);
            } else {
                free.push(words);
            }
        }
        for e in list.iter().filter(|e| !e.carried && e.offset != [0.0; 3]) {
            let words = format!(
                "{} at {} {:.2} kN m",
                model.members[idx].id,
                model.node_name(e.node),
                norm(end_moment(model, idx, e, n)) / 1000.0
            );
            if cable {
                left_out.push(words);
            } else {
                own.push(words);
            }
        }
    }
    let mut text = format!(
        "eccentric member ends (connection shifts): {ends} end(s) of {members} member(s), \
         rigid end offsets to their joints"
    );
    let shown = 12;
    let list = |v: &[String]| {
        format!(
            "{}{}",
            v.iter().take(shown).cloned().collect::<Vec<_>>().join(", "),
            if v.len() > shown { ", …" } else { "" }
        )
    };
    if !own.is_empty() {
        text.push_str(&format!(
            "; at {} end(s) where only bars meet the joint the solve passes the force through \
             the work point and the member's own check takes the moment, uniform along it (on \
             the safe side for it; a frame joint would share it): {}",
            own.len(),
            list(&own)
        ));
    }
    if !left_out.is_empty() {
        text.push_str(&format!(
            "; LEFT OUT at {} cable end(s) where only bars meet the joint (a cable cannot bend; \
             NOT ON THE SAFE SIDE: the moment is missing, so the members' usage there can be too \
             low — make it a frame joint): {}",
            left_out.len(),
            list(&left_out)
        ));
    }
    if !free.is_empty() {
        text.push_str(&format!(
            "; at {} end(s) whose offset's sign is not known the solve leaves it out (at frame \
             joints too) and the member's own check takes the moment whole and sign-free, \
             uniform along it (on the safe side for it; the joint's other members relieved of \
             nothing): {}",
            free.len(),
            list(&free)
        ));
    }
    if !free_left.is_empty() {
        text.push_str(&format!(
            "; LEFT OUT at {} cable end(s) whose offset's sign is not known (a cable cannot \
             bend; NOT ON THE SAFE SIDE: the moment is missing, so the members' usage there can \
             be too low — give the offset's side, or make it a frame joint): {}",
            free_left.len(),
            list(&free_left)
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
