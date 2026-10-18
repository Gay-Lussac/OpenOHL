// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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
//!   (the solver gives them none).
//!
//! A direction along a structure axis is held **exactly**: that degree of
//! freedom is removed, as a support would remove it. A skew one gets a
//! **spring** along it as stiff as the joint's stiffest member (EA/L, or
//! GJ/L for a rotation) — numerically harmless, and, since nothing else
//! acts in that direction, as good as a restraint. Either way the held
//! direction carries only what is loaded along it (a load the joint could
//! not carry anyway); the solver reports that force per stabilisation and
//! refuses one that carries a real share of the loads
//! ([`crate::solve::HELD_SHARE`]). The stabilisation is used only after a
//! solve without it found a zero pivot (or buckling): a model that solves
//! as it is never sees it, and its results stay the same bit for bit.

use nalgebra::{Matrix3, SymmetricEigen, Vector3};

use crate::model::{MemberKind, Model, cross, norm, sub};

/// A direction with less than this share of a joint's largest stiffness
/// (eigenvalue of Σ eeᵀ: the squared sine of the angle out of the plane
/// or the line, so about 0.06°) has none.
const FREE: f64 = 1e-6;
/// A unit direction this close to a structure axis (1 − cos) is that
/// axis.
const ON_AXIS: f64 = 1e-9;

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
    /// one: that degree of freedom is removed. None: a spring.
    pub axis: Option<usize>,
    /// Stiffness of the spring along a skew direction, N/m or N·m/rad (0
    /// for an axis).
    pub stiffness: f64,
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
    let frame = |k: usize| model.members[k].kind == MemberKind::Frame;
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
            });
        }
    }
    out.sort_by_key(|s| (s.node, s.rotation));
    out
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
