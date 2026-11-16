// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The engine additions of calculation level 4 ([`crate::tangent`]): the
//! warm start of the tension-only active set gives the cold start's
//! solution bit for bit (fewer solves), the tangent condensed at a
//! solution is the condensation at the same state bit for bit, matches
//! finite differences of full solves under P-Delta, and on an X panel
//! whose compressed diagonal goes slack is the panel with one diagonal
//! (the method of joints).

use crate::condense::{Attachment, Linearisation, condense};
use crate::model::{JointPoint, Member, MemberKind, Model, NodalLoad, Node, Section};
use crate::solve::{Loads, Options, default_budget, solve, solve_from};
use crate::tangent::{AT_SOLUTION, FullModel, condensed_tangent};

const E: f64 = 2.1e11;

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs().max(1e-30)
}

/// A vertical frame column fixed at its foot, nodes at `heights`, an
/// attachment point "top" at its top.
fn column(heights: &[f64], a: f64, i: f64) -> Model {
    let mut m = Model {
        sections: vec![Section::elastic("s", E, a, i)],
        ..Model::default()
    };
    m.nodes.push(Node::fixed("base", [0.0, 0.0, 0.0]));
    for (k, h) in heights.iter().enumerate() {
        m.nodes
            .push(Node::free(&format!("n{}", k + 1), [0.0, 0.0, *h]));
        m.members.push(Member::new(
            &format!("m{k}"),
            MemberKind::Frame,
            k,
            k + 1,
            0,
        ));
    }
    m.joint_points.push(JointPoint {
        label: "top".into(),
        node: heights.len(),
        offset: [0.0; 3],
    });
    m
}

/// A panel 4 m wide and 3 m high in the x–z plane, feet pinned, legs and
/// top trusses, the diagonals AC and BD tension-only (no second moment:
/// slack in compression), attachment point "D" at the top joint D, held
/// across its plane by a spring at C and D (so no stabilisation).
fn panel() -> Model {
    let mut m = Model {
        sections: vec![Section::elastic("s", E, 1e-3, 0.0)],
        ..Model::default()
    };
    m.nodes = vec![
        Node::pinned("A", [0.0, 0.0, 0.0]),
        Node::pinned("B", [4.0, 0.0, 0.0]),
        Node::free("C", [4.0, 0.0, 3.0]),
        Node::free("D", [0.0, 0.0, 3.0]),
    ];
    for n in [2, 3] {
        m.nodes[n].spring[1] = 1e7;
    }
    m.members = vec![
        Member::new("AD", MemberKind::Truss, 0, 3, 0),
        Member::new("BC", MemberKind::Truss, 1, 2, 0),
        Member::new("DC", MemberKind::Truss, 3, 2, 0),
        Member::new("AC", MemberKind::Tension, 0, 2, 0),
        Member::new("BD", MemberKind::Tension, 1, 3, 0),
    ];
    m.joint_points.push(JointPoint {
        label: "D".into(),
        node: 3,
        offset: [0.0; 3],
    });
    m
}

fn at(node: usize, force: [f64; 3]) -> Loads {
    Loads {
        nodal: vec![NodalLoad {
            node,
            force,
            moment: [0.0; 3],
        }],
        ..Loads::default()
    }
}

/// The warm start: from the set the cold start settles on, the same
/// solution bit for bit in fewer solves; from the opposite set (the other
/// diagonal out), it settles back on the same; a start of another length
/// is the cold start; P-Delta too.
#[test]
fn the_warm_start_gives_the_cold_solution_bit_for_bit() {
    let m = panel();
    for nonlinear in [false, true] {
        let options = Options {
            nonlinear,
            ..Options::default()
        };
        let loads = at(3, [10e3, 0.0, -5e3]);
        let cold = solve(&m, &loads, &options, &default_budget()).unwrap();
        let set: Vec<bool> = cold.members.iter().map(|f| f.slack || f.bowed).collect();
        assert_eq!(set, [false, false, false, false, true], "BD slack");
        let warm = solve_from(&m, &loads, &options, &default_budget(), Some(&set)).unwrap();
        assert!(warm.passes < cold.passes, "{} {}", warm.passes, cold.passes);
        let same = |a: &crate::Solution, b: &crate::Solution| {
            assert_eq!(a.displacements, b.displacements);
            assert_eq!(a.members, b.members);
            assert_eq!(a.reactions, b.reactions);
        };
        same(&warm, &cold);
        let opposite = [false, false, false, true, false];
        let back = solve_from(&m, &loads, &options, &default_budget(), Some(&opposite)).unwrap();
        same(&back, &cold);
        let none = solve_from(&m, &loads, &options, &default_budget(), Some(&[true])).unwrap();
        assert_eq!(none, cold);
        let cold_again = solve_from(&m, &loads, &options, &default_budget(), None).unwrap();
        assert_eq!(cold_again, cold);
    }
}

/// The tangent at a solution is the condensation at the same state, bit
/// for bit: at the unloaded first-order solution, the unloaded
/// condensation; at a P-Delta solution, the condensation at a reference
/// with those loads.
#[test]
fn the_tangent_at_a_solution_is_the_condensation_there() {
    let (l, a, i) = (12.0, 4e-3, 3e-5);
    let m = column(&[4.0, 8.0, l], a, i);
    let points = [Attachment::of_type("top")];
    let budget = default_budget();
    let unloaded = solve(
        &m,
        &Loads::default(),
        &Options {
            nonlinear: false,
            ..Options::default()
        },
        &budget,
    )
    .unwrap();
    let mut c = condensed_tangent(&m, &points, &unloaded, &budget).unwrap();
    assert_eq!(c.linearisation, AT_SOLUTION);
    let u = condense(&m, &points, &Linearisation::Unloaded, &budget).unwrap();
    c.linearisation = u.linearisation;
    assert_eq!(c, u);
    let loads = at(3, [2e3, 0.0, -40e3]);
    let s = solve(&m, &loads, &Options::default(), &budget).unwrap();
    let mut c = condensed_tangent(&m, &points, &s, &budget).unwrap();
    let r = condense(&m, &points, &Linearisation::Tangent(loads), &budget).unwrap();
    c.linearisation = r.linearisation;
    assert_eq!(c, r);
}

/// Under P-Delta (a column at a quarter of its buckling load, pushed
/// sideways), the full model's displacement at its point is the solve's,
/// and its tangent the finite difference of two full solves around it;
/// first order the tangent is the elastic cantilever's L³/3EI.
#[test]
fn the_full_solve_and_its_tangent_against_finite_differences() {
    let (l, a, i) = (12.0, 4e-3, 3e-5);
    let mut m = column(&[4.0, 8.0, l], a, i);
    m.analysis.nonlinear = true;
    m.analysis.self_weight = false;
    let full = FullModel::of_model(m.clone(), vec![Attachment::of_type("top")]);
    let budget = default_budget();
    let p = -0.25 * std::f64::consts::PI.powi(2) * E * i / (4.0 * l * l);
    let s = full.solve(&[[1e3, 0.0, p]], &[], None, &budget).unwrap();
    let direct = solve(&m, &at(3, [1e3, 0.0, p]), &Options::default(), &budget).unwrap();
    assert_eq!(
        s.displacement[0],
        [0, 1, 2].map(|k| direct.displacements[3][k])
    );
    let h = 10.0;
    let x = |f: f64| {
        full.solve(&[[1e3 + f, 0.0, p]], &[], None, &budget)
            .unwrap()
            .displacement[0][0]
    };
    let fd = (x(h) - x(-h)) / (2.0 * h);
    assert!(
        close(s.tangent.flexibility[(0, 0)], fd, 1e-5),
        "tangent {:e} against finite differences {fd:e}",
        s.tangent.flexibility[(0, 0)]
    );
    let elastic = l.powi(3) / (3.0 * E * i);
    assert!(s.tangent.flexibility[(0, 0)] / elastic > 1.25);
    let mut linear = m;
    linear.analysis.nonlinear = false;
    let first = FullModel::of_model(linear, vec![Attachment::of_type("top")]);
    let s = first.solve(&[[1e3, 0.0, p]], &[], None, &budget).unwrap();
    assert!(close(s.tangent.flexibility[(0, 0)], elastic, 1e-9));
    assert!(close(s.displacement[0][0], 1e3 * elastic, 1e-9));
}

/// An X panel loaded sideways at D: its compressed diagonal BD slack at
/// the solution, the tangent the panel with AC alone — at D along x, by
/// the method of joints, Σ N_k n_k L_k / EA over the legs, the top and AC
/// (unit force at D along x: DC −1, AC 5/4, BC −3/4, AD 0; first order)
/// — softer than
/// the unloaded panel (both diagonals), which a load the other way
/// leaves with AC slack instead.
#[test]
fn an_x_panel_is_softer_with_its_slack_diagonal_out() {
    // First order: the hand solution has no geometric stiffness.
    let mut m = panel();
    m.analysis.nonlinear = false;
    let full = FullModel::of_model(m.clone(), vec![Attachment::of_type("D")]);
    let budget = default_budget();
    let s = full.solve(&[[10e3, 0.0, 0.0]], &[], None, &budget).unwrap();
    assert_eq!(s.out, 1);
    assert!(s.inactive[4], "BD slack");
    let ea = E * 1e-3;
    let hand = (1.0 * 4.0 + 1.25f64.powi(2) * 5.0 + 0.75f64.powi(2) * 3.0) / ea;
    assert!(
        close(s.tangent.flexibility[(0, 0)], hand, 1e-9),
        "{:e} against {hand:e}",
        s.tangent.flexibility[(0, 0)]
    );
    let unloaded = condense(
        &m,
        &[Attachment::of_type("D")],
        &Linearisation::Unloaded,
        &budget,
    )
    .unwrap();
    assert!(unloaded.flexibility[(0, 0)] < 0.7 * hand);
    // The warm start from that set the other way round: AC slack.
    let back = full
        .solve(&[[-10e3, 0.0, 0.0]], &[], Some(&s.inactive), &budget)
        .unwrap();
    assert!(back.inactive[3] && !back.inactive[4]);
}
