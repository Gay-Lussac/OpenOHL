// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Reactions at sprung joints: a spring's force −k·u is its joint's
//! reaction, so the reactions balance the loads with springs as with
//! supports.

use super::*;

const E: f64 = 2.1e11;

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs().max(1e-30)
}

fn first_order() -> Options {
    Options {
        nonlinear: false,
        ..Options::default()
    }
}

fn load(node: usize, force: [f64; 3]) -> NodalLoad {
    NodalLoad {
        node,
        force,
        moment: [0.0; 3],
    }
}

fn solved(m: &Model, nodal: Vec<NodalLoad>, self_weight: f64, options: &Options) -> Solution {
    let loads = Loads { nodal, self_weight };
    solve(m, &loads, options, &default_budget()).unwrap_or_else(|e| panic!("{e}"))
}

fn reaction(s: &Solution, node: usize) -> Reaction {
    *s.reactions
        .iter()
        .find(|r| r.node == node)
        .unwrap_or_else(|| panic!("no reaction at node {node}: {:?}", s.reactions))
}

/// Σ reactions + Σ loads (the self-weight included) per direction.
fn out_of_balance(m: &Model, s: &Solution, nodal: &[NodalLoad], self_weight: f64) -> [f64; 3] {
    let mut sum = [0.0, 0.0, -self_weight * m.weight()];
    let forces = nodal.iter().map(|l| l.force);
    for f in forces.chain(s.reactions.iter().map(|r| r.force)) {
        for (a, b) in sum.iter_mut().zip(f) {
            *a += b;
        }
    }
    sum
}

/// A 4 m beam along x: A pinned (and held about x, so the beam cannot
/// spin), C a joint 1 m from A, B on a vertical spring of `k` (held
/// across the beam in y). Statically determinate: P at C puts P·3/4 on A
/// and P/4 on B, whatever the spring.
fn beam_on_spring(k: f64) -> Model {
    let mut m = Model {
        sections: vec![Section::elastic("s", E, 1e-3, 2e-6)],
        ..Model::default()
    };
    let mut a = Node::pinned("A", [0.0, 0.0, 0.0]);
    a.fixed[3] = true;
    let mut b = Node::free("B", [4.0, 0.0, 0.0]);
    b.fixed[1] = true;
    b.spring[2] = k;
    m.nodes = vec![a, Node::free("C", [1.0, 0.0, 0.0]), b];
    m.members = vec![
        Member::new("AC", MemberKind::Frame, 0, 1, 0),
        Member::new("CB", MemberKind::Frame, 1, 2, 0),
    ];
    m
}

/// The spring's force is the reaction statics gives, and −k·u; the
/// reactions balance the load, first order and with P-Delta.
#[test]
fn a_springs_force_is_its_joints_reaction() {
    let (p, k) = (10e3, 1e6);
    let m = beam_on_spring(k);
    let nodal = vec![load(1, [0.0, 0.0, -p])];
    for options in [first_order(), Options::default()] {
        let s = solved(&m, nodal.clone(), 0.0, &options);
        let b = reaction(&s, 2);
        assert!(close(b.force[2], p / 4.0, 1e-9), "{b:?}");
        assert!(close(b.force[2], -k * s.displacements[2][2], 1e-9), "{b:?}");
        assert!(close(s.displacements[2][2], -p / 4.0 / k, 1e-9));
        let a = reaction(&s, 0);
        assert!(close(a.force[2], 0.75 * p, 1e-9), "{a:?}");
        let ob = out_of_balance(&m, &s, &nodal, 0.0);
        assert!(ob.iter().all(|x| x.abs() < 1e-9 * p), "{ob:?}");
    }
}

/// A cantilever on a rotational spring kr about y at its base (the base
/// otherwise fixed): the spring takes the base moment P·L, turning by
/// P·L/kr; the tip moves by PL³/3EI plus that rotation times L.
#[test]
fn a_sprung_rotation_takes_the_moment() {
    let (l, p, kr) = (4.0, 1e3, 5e5);
    let s0 = Section::elastic("s", E, 1e-3, 2e-6);
    let i = s0.iz;
    let mut m = Model {
        sections: vec![s0],
        ..Model::default()
    };
    let mut base = Node::fixed("base", [0.0, 0.0, 0.0]);
    base.fixed[4] = false;
    base.spring[4] = kr;
    m.nodes = vec![base, Node::free("tip", [l, 0.0, 0.0])];
    m.members = vec![Member::new("beam", MemberKind::Frame, 0, 1, 0)];
    let nodal = vec![load(1, [0.0, 0.0, -p])];
    for options in [first_order(), Options::default()] {
        let s = solved(&m, nodal.clone(), 0.0, &options);
        let r = reaction(&s, 0);
        // The load's moment about the base, −P·L about y, held by the
        // spring's −kr·θ.
        assert!(close(r.moment[1], -p * l, 1e-9), "{r:?}");
        assert!(close(r.moment[1], -kr * s.displacements[0][4], 1e-9));
        assert!(close(r.force[2], p, 1e-9), "{r:?}");
        let theta = p * l / kr;
        assert!(close(s.displacements[0][4], theta, 1e-9));
        assert!(close(
            s.displacements[1][2],
            -(p * l.powi(3) / (3.0 * E * i) + theta * l),
            1e-9
        ));
    }
}

/// A support and a spring at one joint: in one direction the support
/// takes it all (the joint does not move, the spring carries nothing);
/// in another, the spring takes its share.
#[test]
fn a_support_and_a_spring_at_one_joint() {
    let (p, k) = (10e3, 1e6);
    let mut m = beam_on_spring(k);
    // A: pinned and sprung along z too, and sprung along y beside B's
    // support across the beam.
    m.nodes[0].spring = [0.0, 2e6, 3e6, 0.0, 0.0, 0.0];
    m.nodes[0].fixed[1] = false;
    let nodal = vec![load(1, [0.0, 2e3, -p])];
    for options in [first_order(), Options::default()] {
        let s = solved(&m, nodal.clone(), 0.0, &options);
        let (a, b) = (reaction(&s, 0), reaction(&s, 2));
        assert_eq!(s.displacements[0][2], 0.0);
        assert!(close(a.force[2], 0.75 * p, 1e-9), "{a:?}");
        assert!(close(b.force[2], p / 4.0, 1e-9), "{b:?}");
        // Across the beam: A's spring and B's support share 2 kN as the
        // beam's statics give (3/4 on A, 1/4 on B).
        assert!(close(a.force[1], -1.5e3, 1e-9), "{a:?}");
        assert!(close(a.force[1], -2e6 * s.displacements[0][1], 1e-9));
        assert!(close(b.force[1], -0.5e3, 1e-9), "{b:?}");
        let ob = out_of_balance(&m, &s, &nodal, 0.0);
        assert!(ob.iter().all(|x| x.abs() < 1e-9 * p), "{ob:?}");
    }
}

/// A portal frame standing on springs only (every direction at both
/// feet, translations and rotations), loaded sideways and down and by
/// its own weight: the reactions — all of them springs' — balance the
/// loads, first order and with P-Delta.
#[test]
fn a_frame_on_springs_is_in_balance() {
    let mut s0 = Section::elastic("s", E, 4e-3, 2e-5);
    s0.density = 7850.0;
    let mut m = Model {
        sections: vec![s0],
        ..Model::default()
    };
    let foot = |id: &str, x: f64| {
        let mut n = Node::free(id, [x, 0.0, 0.0]);
        n.spring = [2e7, 3e7, 5e7, 4e6, 6e6, 1e6];
        n
    };
    m.nodes = vec![
        foot("A", 0.0),
        Node::free("C", [0.0, 0.0, 6.0]),
        Node::free("D", [5.0, 0.0, 6.0]),
        foot("B", 5.0),
    ];
    m.members = vec![
        Member::new("AC", MemberKind::Frame, 0, 1, 0),
        Member::new("CD", MemberKind::Frame, 1, 2, 0),
        Member::new("DB", MemberKind::Frame, 2, 3, 0),
    ];
    let nodal = vec![load(1, [20e3, 3e3, -150e3]), load(2, [0.0, 0.0, -150e3])];
    for options in [first_order(), Options::default()] {
        let s = solved(&m, nodal.clone(), 1.0, &options);
        assert_eq!(s.reactions.len(), 2);
        for r in &s.reactions {
            let u = s.displacements[r.node];
            let k = m.nodes[r.node].spring;
            for d in 0..3 {
                assert!(close(r.force[d], -k[d] * u[d], 1e-6), "{r:?} {u:?}");
                assert!(
                    close(r.moment[d], -k[3 + d] * u[3 + d], 1e-6),
                    "{r:?} {u:?}"
                );
            }
        }
        let ob = out_of_balance(&m, &s, &nodal, 1.0);
        assert!(ob.iter().all(|x| x.abs() < 1e-6 * 300e3), "{ob:?}");
    }
}
