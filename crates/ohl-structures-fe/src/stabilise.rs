// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Joint stabilisation: the directions in which a joint has (nearly) no
//! stiffness of its own, found from the model's geometry, and held so the
//! stiffness matrix can be factorised (the way tower programs stabilise the
//! joints of towers drawn as trusses).
//!
//! - **Translations** of a joint with no frame member: the directions its
//!   truss and cable members, supports and springs do not reach — normal
//!   to the plane of coplanar members, normal to the line of collinear
//!   ones. Found from the eigenvalues of Σ eeᵀ over the unit directions e
//!   of the members (and of the supported or sprung axes).
//! - **Rotations** of a chain of collinear frame members about their
//!   common axis, when no joint of the chain has a frame in another
//!   direction or a rotational support or spring about that axis: only
//!   the torsion of the members holds them, and it holds them against
//!   each other only (the chain spins freely). One joint of the chain is
//!   held. Joints with truss members only have no rotations at all
//!   (the solver gives them none). A tension-only member that bends
//!   ([`crate::Member::bending`]) counts as a frame while it is in the
//!   model, and as nothing while it is left out.
//!
//! A translation of a joint that lies on straight lines of members whose
//! ends are held by the structure (a joint in a face of a tower, on a
//! member cut there — a redundant's, a K's or a plan bracing's joint) is
//! held **relative to those ends** ([`Stabilisation::relative`]): the
//! joint follows, along its free direction, the point of each line where
//! it lies (interpolated between the line's ends), as the continuous
//! member it cuts would carry it; with several lines, their mean. A
//! line's ends are the first joints along it, through joints that
//! continue it straight, that are not held themselves, or are held
//! relative to others already (so a chain of such holds ends at held
//! joints, never in a loop). Other free translations, and every
//! rotation, are held **to the ground**: a direction along a structure
//! axis exactly (that degree of freedom is removed, as a support would
//! remove it), a skew one by a **spring** along it as stiff as the
//! joint's stiffest member (EA/L, or GJ/L for a rotation) — numerically
//! harmless, and, since nothing else acts in that direction, as good as a
//! restraint. A relative hold is a spring of the same stiffness on the
//! joint's motion less its lines' (k·(n·u_J − Σ w n·u_e)²/2), so what it
//! takes at the joint goes to the lines' ends, not to the ground: in
//! first order the load along the direction (to the ends, as the cut
//! member would carry it), with P-Delta also the pull of its members'
//! geometric stiffness (N/L), which is ~0 when the joint follows its
//! face. Either way the held direction carries only what is loaded along
//! it (a load the joint could not carry anyway); the solver reports that
//! force per stabilisation and refuses one that carries a real share of
//! the loads ([`crate::solve::HELD_SHARE`]). The stabilisation is used
//! only after a solve without it found a zero pivot (or buckling): a
//! model that solves as it is never sees it, and its results stay the
//! same bit for bit.

use nalgebra::{Matrix3, SymmetricEigen, Vector3};

use crate::model::{Model, cross, norm, sub};

/// A direction with less than this share of a joint's largest stiffness
/// (eigenvalue of Σ eeᵀ: the squared sine of the angle out of the plane
/// or the line, so about 0.06°) has none.
const FREE: f64 = 1e-6;
/// A unit direction this close to a structure axis (1 − cos) is that
/// axis.
const ON_AXIS: f64 = 1e-9;
/// Members at a joint whose directions differ by less than this (the
/// sine of the angle, about 0.6°) continue one straight line (a member
/// cut there; the joints of generated bodies are rounded to the
/// millimetre, a few tenths of a millimetre off the line over a metre).
const STRAIGHT: f64 = 1e-2;

/// A direction of a joint held by the solver.
#[derive(Debug, Clone, PartialEq)]
pub struct Stabilisation {
    /// Node (index).
    pub node: usize,
    /// A rotation (else a translation).
    pub rotation: bool,
    /// Unit direction (of the translation, or the rotation's axis),
    /// structure axes.
    pub direction: [f64; 3],
    /// The structure axis (0 x, 1 y, 2 z) when the direction is along
    /// one: held to the ground, that degree of freedom is removed. None:
    /// a spring.
    pub axis: Option<usize>,
    /// Stiffness of the spring, N/m or N·m/rad (0 for an axis held to the
    /// ground).
    pub stiffness: f64,
    /// Held relative to these joints (index, weight; the weights sum to
    /// 1): the spring acts on the joint's motion along the direction less
    /// the weighted motion of the joints — the ends of the straight lines
    /// of members it lies on (module docs). Empty: held to the ground.
    pub relative: Vec<(usize, f64)>,
}

impl Stabilisation {
    /// The degree of freedom in words: `y`, `rz`, or the direction
    /// (`along (0.000, 0.995, 0.100)`, `about (…)`).
    pub fn dof_name(&self) -> String {
        match self.axis {
            Some(a) => crate::solve::DOF_NAMES[a + if self.rotation { 3 } else { 0 }].into(),
            None => {
                let d = self.direction;
                format!(
                    "{} ({:.3}, {:.3}, {:.3})",
                    if self.rotation { "about" } else { "along" },
                    d[0],
                    d[1],
                    d[2]
                )
            }
        }
    }
}

/// A held direction of a solve and the force (or moment) it took on the
/// structure along its direction, N or N·m.
#[derive(Debug, Clone, PartialEq)]
pub struct Stabilised {
    /// The stabilisation.
    pub at: Stabilisation,
    /// Force (rotations: moment) on the structure along the direction.
    pub force: f64,
}

/// The stabilisations of `model`: every joint direction without stiffness
/// of its own (module docs), in node order. `inactive`: per member, left
/// out of the model (a tension-only member gone slack; empty: none) — a
/// joint whose members are all inactive is held in every direction its
/// supports do not hold.
pub(crate) fn find(model: &Model, inactive: &[bool]) -> Vec<Stabilisation> {
    let n_nodes = model.nodes.len();
    let mut at_node: Vec<Vec<usize>> = vec![Vec::new(); n_nodes];
    // Joints with members, active or not (they have equations).
    let mut connected = vec![false; n_nodes];
    for (k, m) in model.members.iter().enumerate() {
        connected[m.i] = true;
        connected[m.j] = true;
        if inactive.get(k).copied().unwrap_or(false) {
            continue;
        }
        at_node[m.i].push(k);
        at_node[m.j].push(k);
    }
    let unit = |k: usize| -> Vector3<f64> {
        let m = &model.members[k];
        let d = sub(model.nodes[m.j].point, model.nodes[m.i].point);
        let l = norm(d);
        Vector3::new(d[0] / l, d[1] / l, d[2] / l)
    };
    let frame = |k: usize| model.members[k].is_frame();
    let mut out = Vec::new();
    // Translations of joints without a frame.
    for (n, members) in at_node.iter().enumerate() {
        let node = &model.nodes[n];
        if members.is_empty() {
            // Every member inactive: each axis its supports leave free.
            if connected[n] {
                for d in (0..3).filter(|&d| !node.fixed[d] && node.spring[d] <= 0.0) {
                    let mut direction = [0.0; 3];
                    direction[d] = 1.0;
                    out.push(Stabilisation {
                        node: n,
                        rotation: false,
                        direction,
                        axis: Some(d),
                        stiffness: 0.0,
                        relative: Vec::new(),
                    });
                }
            }
            continue;
        }
        if members.iter().any(|&k| frame(k)) {
            continue;
        }
        let mut s = Matrix3::zeros();
        let mut stiffest: f64 = 0.0;
        for &k in members {
            let e = unit(k);
            s += e * e.transpose();
            let m = &model.members[k];
            let sec = &model.sections[m.section];
            stiffest = stiffest.max(sec.e * sec.area / model.length(m));
        }
        let held = |d: usize| node.fixed[d] || node.spring[d] > 0.0;
        for d in 0..3 {
            if held(d) {
                s[(d, d)] += 1.0;
            }
        }
        for (direction, axis) in free_directions(s, |d| !held(d)) {
            out.push(Stabilisation {
                node: n,
                rotation: false,
                direction,
                axis,
                stiffness: if axis.is_some() { 0.0 } else { stiffest },
                relative: Vec::new(),
            });
        }
    }
    // Rotations of chains of collinear frames.
    let mut seen = vec![false; n_nodes];
    for start in 0..n_nodes {
        if seen[start] || !at_node[start].iter().any(|&k| frame(k)) {
            continue;
        }
        let first = at_node[start].iter().copied().find(|&k| frame(k));
        let Some(first) = first else { continue };
        let a = unit(first);
        let along = |k: usize| cross_norm(unit(k), a) <= FREE.sqrt();
        // The chain: joints reached through frames along a.
        let mut chain = vec![start];
        seen[start] = true;
        let mut head = 0;
        let mut free = true;
        let mut stiffest: f64 = 0.0;
        while head < chain.len() {
            let n = chain[head];
            head += 1;
            let node = &model.nodes[n];
            for d in 0..3 {
                let restrained = node.fixed[3 + d] || node.spring[3 + d] > 0.0;
                if restrained && a[d].abs() > FREE.sqrt() {
                    free = false;
                }
            }
            for &k in &at_node[n] {
                if !frame(k) {
                    continue;
                }
                if !along(k) {
                    free = false;
                    continue;
                }
                let m = &model.members[k];
                let sec = &model.sections[m.section];
                stiffest = stiffest.max(sec.g * sec.j / model.length(m));
                let other = if m.i == n { m.j } else { m.i };
                if !seen[other] {
                    seen[other] = true;
                    chain.push(other);
                }
            }
        }
        if !free {
            continue;
        }
        // Held at the chain's first joint (in node order).
        let n = chain.iter().copied().min().unwrap_or(start);
        let fixed = model.nodes[n].fixed;
        let s = a * a.transpose();
        for (direction, axis) in free_directions(Matrix3::identity() - s, |d| !fixed[3 + d]) {
            out.push(Stabilisation {
                node: n,
                rotation: true,
                direction,
                axis,
                stiffness: if axis.is_some() { 0.0 } else { stiffest },
                relative: Vec::new(),
            });
        }
    }
    out.sort_by_key(|s| (s.node, s.rotation));
    hold_relative(model, &at_node, &mut out);
    out
}

/// Turns the ground holds of free translations into holds relative to the
/// ends of the straight lines of members through their joint, where those
/// ends are held (module docs). Rounds: a joint whose every line reaches
/// joints held at the start of the round is held relative to them; the
/// joints of a round count as held in the next ones.
fn hold_relative(model: &Model, at_node: &[Vec<usize>], out: &mut [Stabilisation]) {
    let n_nodes = model.nodes.len();
    let mut free = vec![false; n_nodes];
    for h in out.iter().filter(|h| !h.rotation) {
        free[h.node] = true;
    }
    if !free.iter().any(|&f| f) {
        return;
    }
    // Candidates: joints with free translations and no support or spring
    // on a translation (a support holds them to the ground already).
    let candidate = |n: usize| {
        let node = &model.nodes[n];
        free[n] && !at_node[n].is_empty() && (0..3).all(|d| !node.fixed[d] && node.spring[d] <= 0.0)
    };
    let point = |n: usize| Vector3::from(model.nodes[n].point);
    let other = |k: usize, n: usize| {
        let m = &model.members[k];
        if m.i == n { m.j } else { m.i }
    };
    let toward = |n: usize, a: usize| (point(a) - point(n)).normalize();
    let straight =
        |a: Vector3<f64>, b: Vector3<f64>| cross_norm(a, b) <= STRAIGHT && a.dot(&b) > 0.0;
    // Along the line from `n` through member `k`: the first joint held
    // (`held`), through joints that continue the line straight; none when
    // the line stops at a joint that is not held.
    let walk = |n: usize, k: usize, held: &[bool]| -> Option<usize> {
        let dir = toward(n, other(k, n));
        let mut at = other(k, n);
        for _ in 0..n_nodes {
            if held[at] {
                return Some(at);
            }
            let next = at_node[at]
                .iter()
                .map(|&q| other(q, at))
                .find(|&b| straight(toward(at, b), dir))?;
            at = next;
        }
        None
    };
    let mut held: Vec<bool> = (0..n_nodes).map(|n| !free[n]).collect();
    let mut weights: Vec<Option<Vec<(usize, f64)>>> = vec![None; n_nodes];
    loop {
        let mut round = Vec::new();
        for n in (0..n_nodes).filter(|&n| candidate(n) && !held[n]) {
            // The lines through n: pairs of its members pointing opposite
            // ways, each line once.
            let members = &at_node[n];
            let mut lines = Vec::new();
            for (x, &p) in members.iter().enumerate() {
                for &q in &members[x + 1..] {
                    let (a, b) = (toward(n, other(p, n)), toward(n, other(q, n)));
                    if straight(a, -b)
                        && !lines
                            .iter()
                            .any(|&(l, _)| straight(toward(n, other(l, n)), a))
                    {
                        lines.push((p, q));
                    }
                }
            }
            if lines.is_empty() {
                continue;
            }
            let mut w: Vec<(usize, f64)> = Vec::new();
            let mut all = true;
            for &(p, q) in &lines {
                let (Some(a), Some(b)) = (walk(n, p, &held), walk(n, q, &held)) else {
                    all = false;
                    break;
                };
                let (la, lb) = ((point(a) - point(n)).norm(), (point(b) - point(n)).norm());
                let share = 1.0 / lines.len() as f64;
                for (e, v) in [(a, lb / (la + lb)), (b, la / (la + lb))] {
                    match w.iter_mut().find(|(x, _)| *x == e) {
                        Some(x) => x.1 += share * v,
                        None => w.push((e, share * v)),
                    }
                }
            }
            if all {
                round.push((n, w));
            }
        }
        if round.is_empty() {
            break;
        }
        for (n, w) in round {
            held[n] = true;
            weights[n] = Some(w);
        }
    }
    for h in out.iter_mut().filter(|h| !h.rotation) {
        if let Some(w) = &weights[h.node] {
            h.relative = w.clone();
            if h.stiffness == 0.0 {
                h.stiffness = stiffest(model, &at_node[h.node]);
            }
        }
    }
}

/// The largest EA/L of the members `members`.
fn stiffest(model: &Model, members: &[usize]) -> f64 {
    members
        .iter()
        .map(|&k| {
            let m = &model.members[k];
            let sec = &model.sections[m.section];
            sec.e * sec.area / model.length(m)
        })
        .fold(0.0, f64::max)
}

fn cross_norm(a: Vector3<f64>, b: Vector3<f64>) -> f64 {
    norm(cross([a.x, a.y, a.z], [b.x, b.y, b.z]))
}

/// The free directions of the symmetric stiffness pattern `s` (its
/// eigenvectors with an eigenvalue below `FREE` × the largest), each as
/// a structure axis when one lies in the free space (and `may(axis)`),
/// else as unit directions completing them.
fn free_directions(s: Matrix3<f64>, may: impl Fn(usize) -> bool) -> Vec<([f64; 3], Option<usize>)> {
    let eig = SymmetricEigen::new(s);
    let largest = eig.eigenvalues.max();
    if largest.is_nan() || largest <= 0.0 {
        return Vec::new();
    }
    let mut free: Vec<Vector3<f64>> = (0..3)
        .filter(|&c| eig.eigenvalues[c] <= FREE * largest)
        .map(|c| eig.eigenvectors.column(c).into_owned())
        .collect();
    let mut out = Vec::new();
    for d in 0..3 {
        if free.is_empty() || !may(d) {
            continue;
        }
        let e = Vector3::ith(d, 1.0);
        // Share of the axis in the free space.
        let inside: f64 = free.iter().map(|v| v.dot(&e).powi(2)).sum();
        if inside < 1.0 - ON_AXIS {
            continue;
        }
        out.push((e.into(), Some(d)));
        // The rest of the free space, orthogonal to this axis.
        let mut rest: Vec<Vector3<f64>> = Vec::new();
        for v in &free {
            let mut w = v - e * v.dot(&e);
            for r in &rest {
                w -= r * r.dot(&w);
            }
            let l = w.norm();
            if l > 1e-6 {
                rest.push(w / l);
            }
        }
        free = rest;
    }
    out.extend(free.into_iter().map(|v| (v.into(), None)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_directions_of_a_plane_and_a_line() {
        // Members in the x–z plane: free along y, exactly.
        let ex = Vector3::new(1.0, 0.0, 0.0);
        let ez = Vector3::new(0.0, 0.0, 1.0);
        let d = Vector3::new(1.0, 0.0, 1.0).normalize();
        let s = ex * ex.transpose() + ez * ez.transpose() + d * d.transpose();
        assert_eq!(
            free_directions(s, |_| true),
            vec![([0.0, 1.0, 0.0], Some(1))]
        );
        // One member along x: y and z, both exactly.
        let s = ex * ex.transpose();
        assert_eq!(
            free_directions(s, |_| true),
            vec![([0.0, 1.0, 0.0], Some(1)), ([0.0, 0.0, 1.0], Some(2))]
        );
        // A skew plane: one skew direction, its normal.
        let a = Vector3::new(1.0, 0.0, 0.0);
        let b = Vector3::new(0.0, 0.1, 1.0).normalize();
        let s = a * a.transpose() + b * b.transpose();
        let f = free_directions(s, |_| true);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].1, None);
        let n = Vector3::from(f[0].0);
        assert!(n.dot(&a).abs() < 1e-12 && n.dot(&b).abs() < 1e-12);
        // A member along x and one along a skew direction in y–z: none.
        let s = a * a.transpose()
            + b * b.transpose()
            + Vector3::new(0.0, 1.0, 0.0) * Vector3::new(0.0, 1.0, 0.0).transpose();
        assert!(free_directions(s, |_| true).is_empty());
    }
}
