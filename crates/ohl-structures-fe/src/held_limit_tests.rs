// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The refusal limit of the joint stabilisation (owner's decision,
//! 2026-10-19): a hold relative to the ends of its joint's lines is not
//! refused above the limit — its load is carried by the lines' ends,
//! flagged — while a hold to the ground above it is still refused, on
//! invented models.

use crate::{
    FeError, Loads, Member, MemberKind, Model, NodalLoad, Node, Options, Section, Solution,
    default_budget, solve, stabilisation_note,
};

const E: f64 = 2.1e11;
/// The pull along the rod, N.
const H: f64 = 10e3;
/// The load across the rod at its middle, N (10 % of the pull: far above
/// the 1 % limit).
const P: f64 = 1e3;

fn force(node: usize, f: [f64; 3]) -> NodalLoad {
    NodalLoad {
        node,
        force: f,
        moment: [0.0; 3],
    }
}

fn run(m: &Model, nodal: Vec<NodalLoad>, nonlinear: bool) -> Result<Solution, FeError> {
    let loads = Loads {
        nodal,
        self_weight: 0.0,
    };
    let options = Options {
        nonlinear,
        ..Options::default()
    };
    solve(m, &loads, &options, &default_budget())
}

/// A truss rod along x in two halves AM, MB, 1 m and 3 m: A fixed, B fixed
/// but along x. M lies on the straight line A–B and is free across it: held
/// relative to A and B (weights 3/4 and 1/4, the inverse of its distances).
fn rod() -> Model {
    let mut m = Model {
        sections: vec![Section::elastic("s", E, 1e-3, 0.0)],
        ..Model::default()
    };
    let mut b = Node::fixed("B", [4.0, 0.0, 0.0]);
    b.fixed[0] = false;
    m.nodes = vec![
        Node::fixed("A", [0.0, 0.0, 0.0]),
        Node::free("M", [1.0, 0.0, 0.0]),
        b,
    ];
    m.members = vec![
        Member::new("AM", MemberKind::Truss, 0, 1, 0),
        Member::new("MB", MemberKind::Truss, 1, 2, 0),
    ];
    m
}

/// P down at M, 10 % of the loads and all of M's own: above the limit at
/// which a ground hold is refused. Held relative to A and B, it solves:
/// the hold takes P and passes it to the line's ends as a beam pinned at
/// A and B carries a load across it — 3P/4 at A, P/4 at B (statics: the
/// load 1 m from A on a 4 m span; with P-Delta the pull takes 6e-5 of
/// it) — and the reactions balance the loads exactly, first order and
/// with P-Delta; the solution lists the hold as
/// above the limit and the note flags it.
#[test]
fn a_relative_hold_above_the_limit_is_carried_by_its_lines_ends() {
    let m = rod();
    for nonlinear in [false, true] {
        let s = run(
            &m,
            vec![force(2, [H, 0.0, 0.0]), force(1, [0.0, 0.0, -P])],
            nonlinear,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        // With P-Delta the pull's geometric stiffness takes a share of
        // 4H/(3k) = 6.3e-5 (k = EA/1 m, the hold's spring: the joint sits
        // that far off its line): the hold and the ends' split to 1e-4.
        let share_tol = if nonlinear { 1e-4 } else { 1e-9 };
        let held: Vec<_> = s.stabilised.iter().filter(|h| h.at.node == 1).collect();
        assert!(!held.is_empty() && held.iter().all(|h| !h.at.relative.is_empty()));
        let z = held
            .iter()
            .find(|h| h.at.direction[2].abs() > 0.99)
            .expect("M held across the rod in z");
        assert!((z.force.abs() - P).abs() <= share_tol * P, "{}", z.force);
        assert_eq!(s.relative_over.len(), 1, "{:?}", s.relative_over);
        assert_eq!(s.stabilised[s.relative_over[0]].at.node, 1);
        // The ends take the load: 3P/4 at A, P/4 at B; Σ reactions + Σ
        // loads = 0 exactly.
        let r = |n: usize| s.reactions.iter().find(|r| r.node == n).unwrap().force;
        let tol = 1e-9 * H;
        assert!((r(0)[2] - 0.75 * P).abs() <= share_tol * P, "{:?}", r(0));
        assert!((r(2)[2] - 0.25 * P).abs() <= share_tol * P, "{:?}", r(2));
        for k in 0..3 {
            let sum: f64 = s.reactions.iter().map(|x| x.force[k]).sum();
            let load = [H, 0.0, -P][k];
            assert!((sum + load).abs() <= tol, "{k}: {sum}");
        }
        let note = stabilisation_note(&m, &s).unwrap();
        assert!(
            note.contains("LARGE: 1 hold(s) relative to their lines"),
            "{note}"
        );
        assert!(note.contains("M z 1.00 kN"), "{note}");
    }
}

/// A joint on no straight line of members — three horizontal trusses at
/// 120° from M to fixed points — is free in z and held to the ground. P
/// down at M is all of the loads: refused as before, naming M.
#[test]
fn a_ground_hold_above_the_limit_is_still_refused() {
    let mut m = Model {
        sections: vec![Section::elastic("s", E, 1e-3, 0.0)],
        ..Model::default()
    };
    m.nodes = vec![Node::free("M", [0.0, 0.0, 0.0])];
    for (k, angle) in [0.0f64, 120.0, 240.0].iter().enumerate() {
        let a = angle.to_radians();
        m.nodes.push(Node::fixed(
            &format!("F{k}"),
            [2.0 * a.cos(), 2.0 * a.sin(), 0.0],
        ));
        m.members.push(Member::new(
            &format!("T{k}"),
            MemberKind::Truss,
            0,
            k + 1,
            0,
        ));
    }
    let e = run(&m, vec![force(0, [0.0, 0.0, -P])], false).unwrap_err();
    assert!(
        matches!(e, FeError::Held { ref node, .. } if node == "M"),
        "{e}"
    );
    // A small load across it (a share of the self-weight beside a large
    // load in the plane) is still only reported.
    let s = run(&m, vec![force(0, [100.0 * P, 0.0, -0.5 * P])], false)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(s.stabilised.iter().all(|h| h.at.relative.is_empty()));
    assert!(s.relative_over.is_empty());
}
