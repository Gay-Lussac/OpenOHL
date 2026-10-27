// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The solver's guards (2026-10-27), on invented models: a near-mechanism
//! (a joint whose three trusses lie 0.1° off one plane) is refused or
//! held, never returned with absurd displacements; iterations stopped
//! just above the tolerance are accepted with a note.

use ohl_core::budget::Budget;

use crate::{
    FeError, Loads, Member, MemberKind, Model, NodalLoad, Node, Options, Section, convergence_note,
    solve,
};

const E: f64 = 2.1e11;

/// A joint J at the origin held by three trusses (1 m in plan, area
/// 10 cm², 120° apart: no two on one line) to pinned supports, each `deg`
/// below J: their directions lie that far off the horizontal plane, so J
/// has a vertical stiffness ∝ sin² of it.
fn star(deg: f64) -> Model {
    let mut m = Model {
        sections: vec![Section::elastic("s", E, 1e-3, 1e-8)],
        ..Model::default()
    };
    m.nodes.push(Node::free("J", [0.0, 0.0, 0.0]));
    let dz = -deg.to_radians().tan();
    for k in 0..3 {
        let a = (120.0 * k as f64).to_radians();
        m.nodes
            .push(Node::pinned(&format!("S{k}"), [a.cos(), a.sin(), dz]));
        m.members.push(Member::new(
            &format!("m{k}"),
            MemberKind::Truss,
            0,
            k + 1,
            0,
        ));
    }
    m
}

fn at(node: usize, f: [f64; 3]) -> Loads {
    Loads {
        nodal: vec![NodalLoad {
            node,
            force: f,
            moment: [0.0; 3],
        }],
        self_weight: 0.0,
    }
}

fn first_order(stabilise: bool) -> Options {
    Options {
        nonlinear: false,
        stabilise,
        ..Options::default()
    }
}

/// Loaded across the near-plane, the joint would move metres (∝ 1/sin²):
/// refused as implausible, naming it, instead of returned; stabilised at
/// the looser threshold, the hold would carry the whole load: refused as
/// a mechanism.
#[test]
fn a_near_mechanism_is_not_returned() {
    let m = star(0.1);
    // 1.5 EA/L sin² ≈ 1 kN/m vertically: 10 kN moves J about 10 m in a
    // structure 2 m across.
    let loads = at(0, [0.0, 0.0, -10e3]);
    let e = solve(&m, &loads, &first_order(false), &crate::default_budget()).unwrap_err();
    match &e {
        FeError::Implausible {
            joints,
            moving,
            largest,
            ..
        } => {
            assert_eq!(joints, &["J".to_owned()]);
            assert_eq!(*moving, 1);
            assert!(*largest > 1.0, "{largest}");
        }
        other => panic!("{other}"),
    }
    assert!(e.to_string().contains("J"), "{e}");
    let e = solve(&m, &loads, &first_order(true), &crate::default_budget()).unwrap_err();
    assert!(matches!(e, FeError::Held { .. }), "{e}");
}

/// Loaded nearly in the plane (the hold takes 0.6 % of the loads), the
/// joint is held at the looser threshold and the solution says so; a
/// plausible model solves as before.
#[test]
fn a_near_mechanism_loaded_in_its_plane_is_held() {
    let m = star(0.1);
    let loads = at(0, [500e3, 0.0, -3e3]);
    let s = solve(&m, &loads, &first_order(true), &crate::default_budget()).unwrap();
    assert!(s.rounded_stabilisation);
    assert_eq!(s.stabilised.len(), 1);
    assert!(
        s.displacements[0][2].abs() < 1e-3,
        "{:?}",
        s.displacements[0]
    );
    let note = convergence_note(&s).unwrap();
    assert!(note.contains("0.2°"), "{note}");
    let ok = star(5.0);
    let s = solve(&ok, &loads, &first_order(true), &crate::default_budget()).unwrap();
    assert!(!s.rounded_stabilisation && s.stabilised.is_empty());
    assert!(convergence_note(&s).is_none());
}

/// A portal frame swaying under heavy vertical loads, P-Delta with a
/// tolerance it cannot reach (10⁻³⁰): stopped by the iteration limit with
/// the forces out of balance at round-off, the solve is accepted, flagged
/// and noted; stopped after one iteration (far from balance), refused.
#[test]
fn p_delta_stopped_just_above_the_tolerance_is_accepted() {
    let mut m = Model {
        sections: vec![Section::elastic("s", E, 2e-3, 4e-6)],
        ..Model::default()
    };
    m.nodes = vec![
        Node::fixed("A", [0.0, 0.0, 0.0]),
        Node::fixed("B", [4.0, 0.0, 0.0]),
        Node::free("C", [0.0, 0.0, 5.0]),
        Node::free("D", [4.0, 0.0, 5.0]),
    ];
    for (id, i, j) in [("AC", 0, 2), ("BD", 1, 3), ("CD", 2, 3)] {
        m.members.push(Member::new(id, MemberKind::Frame, i, j, 0));
    }
    let loads = Loads {
        nodal: vec![
            NodalLoad {
                node: 2,
                force: [2e3, 0.0, -30e3],
                moment: [0.0; 3],
            },
            NodalLoad {
                node: 3,
                force: [0.0, 0.0, -30e3],
                moment: [0.0; 3],
            },
        ],
        self_weight: 0.0,
    };
    let options = Options {
        tolerance: 1e-30,
        ..Options::default()
    };
    let s = solve(&m, &loads, &options, &Budget::new().max_iterations(40)).unwrap();
    assert!(s.stalled);
    assert!(s.residual <= crate::solve::STALL_ACCEPTED, "{}", s.residual);
    let note = convergence_note(&s).unwrap();
    assert!(note.contains("accepted"), "{note}");
    // The same within the tolerance: not flagged.
    let s = solve(&m, &loads, &Options::default(), &crate::default_budget()).unwrap();
    assert!(!s.stalled && convergence_note(&s).is_none());
    // Stopped far from balance: refused.
    match solve(&m, &loads, &options, &Budget::new().max_iterations(1)) {
        Err(FeError::Stopped { residual, .. }) => {
            assert!(residual > crate::solve::STALL_ACCEPTED, "{residual}")
        }
        other => panic!("{other:?}"),
    }
}
