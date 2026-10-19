// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Joint stabilisation on invented models: a planar tower face, a frame
//! column free to spin, mechanisms that stay mechanisms, loads in a
//! direction no member holds.

use crate::{
    FeError, Loads, Member, MemberKind, Model, NodalLoad, Node, Options, Section, Solution,
    default_budget, solve, stabilisation_note,
};

const E: f64 = 2.1e11;

fn first_order(stabilise: bool) -> Options {
    Options {
        nonlinear: false,
        stabilise,
        ..Options::default()
    }
}

fn nodal(loads: Vec<NodalLoad>, self_weight: f64) -> Loads {
    Loads {
        nodal: loads,
        self_weight,
    }
}

fn force(node: usize, f: [f64; 3]) -> NodalLoad {
    NodalLoad {
        node,
        force: f,
        moment: [0.0; 3],
    }
}

/// Turns `p` about the x axis by `angle`.
fn turn(p: [f64; 3], angle: f64) -> [f64; 3] {
    let (s, c) = angle.sin_cos();
    [p[0], c * p[1] - s * p[2], s * p[1] + c * p[2]]
}

/// A planar K-braced tower face in the x–z plane turned about x by
/// `angle`: legs from ±3 m at the ground to ±1 m at 12 m, horizontals at
/// 4, 8 and 12 m split at their middle joints, the diagonals from the
/// corners below to them; pinned feet. Every joint has its members in the
/// face's plane: no stiffness across it.
fn face(angle: f64) -> Model {
    let mut steel = Section::elastic("s", E, 1.2e-3, 0.0);
    steel.density = 7850.0;
    steel.yield_strength = Some(355e6);
    let mut m = Model {
        sections: vec![steel],
        ..Model::default()
    };
    let w = |z: f64| 3.0 - z / 6.0;
    for k in 0..4 {
        let z = 4.0 * k as f64;
        let mk = |id: String, x: f64| {
            let p = turn([x, 0.0, z], angle);
            if k == 0 {
                Node::pinned(&id, p)
            } else {
                Node::free(&id, p)
            }
        };
        m.nodes.push(mk(format!("L{k}"), -w(z)));
        m.nodes.push(mk(format!("R{k}"), w(z)));
        if k > 0 {
            m.nodes.push(mk(format!("M{k}"), 0.0));
        }
    }
    let id = |m: &Model, s: &str| m.nodes.iter().position(|n| n.id == s).unwrap();
    let mut pairs = Vec::new();
    for k in 0..3 {
        let (a, b) = (k, k + 1);
        pairs.push((format!("L{a}"), format!("L{b}")));
        pairs.push((format!("R{a}"), format!("R{b}")));
        pairs.push((format!("L{b}"), format!("M{b}")));
        pairs.push((format!("M{b}"), format!("R{b}")));
        pairs.push((format!("L{a}"), format!("M{b}")));
        pairs.push((format!("R{a}"), format!("M{b}")));
    }
    for (a, b) in pairs {
        let (i, j) = (id(&m, &a), id(&m, &b));
        let name = format!("{a}-{b}");
        m.members
            .push(Member::new(&name, MemberKind::Truss, i, j, 0));
    }
    m
}

/// In-plane loads at the top corners of a face turned by `angle`.
fn top_loads(m: &Model, angle: f64) -> Vec<NodalLoad> {
    ["L3", "R3"]
        .iter()
        .map(|s| {
            let n = m.nodes.iter().position(|n| n.id == *s).unwrap();
            force(n, turn([5e3, 0.0, -10e3], angle))
        })
        .collect()
}

fn solved(m: &Model, loads: &Loads, options: &Options) -> Solution {
    solve(m, loads, options, &default_budget()).unwrap_or_else(|e| panic!("{e}"))
}

/// A planar truss face is a mechanism without stabilisation (as before);
/// with it, it solves exactly as the same face with every joint
/// supported across the plane (the hand model of a plane truss): the same
/// equations, so the same numbers.
#[test]
fn a_planar_tower_face_solves_as_a_plane_truss() {
    let m = face(0.0);
    let loads = nodal(top_loads(&m, 0.0), 0.0);
    let e = solve(&m, &loads, &first_order(false), &default_budget()).unwrap_err();
    assert!(matches!(e, FeError::Mechanism { .. }), "{e}");
    let s = solved(&m, &loads, &first_order(true));
    // Every joint but the feet (held by their supports) held along y.
    assert_eq!(s.stabilised.len(), 9);
    assert!(
        s.stabilised
            .iter()
            .all(|h| h.at.axis == Some(1) && !h.at.rotation)
    );
    assert!(s.stabilised.iter().all(|h| h.force.abs() < 1e-9));
    let mut plane = m.clone();
    for n in &mut plane.nodes {
        n.fixed[1] = true;
    }
    let p = solved(&plane, &loads, &first_order(false));
    assert!(p.stabilised.is_empty());
    assert_eq!(s.displacements, p.displacements);
    for (a, b) in s.members.iter().zip(&p.members) {
        assert_eq!(a.axial, b.axial);
    }
    for (a, b) in s.reactions.iter().zip(&p.reactions) {
        assert_eq!(a.force[0], b.force[0]);
        assert_eq!(a.force[2], b.force[2]);
    }
    // The feet carry the loads: 10 kN across, 20 kN down.
    let sum: [f64; 3] = [0, 1, 2].map(|k| s.reactions.iter().map(|r| r.force[k]).sum());
    assert!(
        (sum[0] + 10e3).abs() < 1e-6 && (sum[2] - 20e3).abs() < 1e-6,
        "{sum:?}"
    );
    let note = stabilisation_note(&m, &s).unwrap();
    assert!(
        note.starts_with("stabilised 9 joint direction(s)"),
        "{note}"
    );
    assert!(note.contains("L1 y"), "{note}");
}

/// A battered face (its plane tilted 10° from vertical): the free
/// direction is skew, held by a spring along it. With loads in the plane
/// the member forces are those of the upright face (turned), and those of
/// a 3D-braced equivalent (every joint tied across the plane by a member
/// to a fixed point), to rounding; P-Delta too.
#[test]
fn a_battered_face_equals_its_braced_equivalent() {
    let angle = 10f64.to_radians();
    let upright = face(0.0);
    let m = face(angle);
    let loads = nodal(top_loads(&m, angle), 0.0);
    let s = solved(&m, &loads, &first_order(true));
    assert_eq!(s.stabilised.len(), 9);
    assert!(s.stabilised.iter().all(|h| h.at.axis.is_none()));
    let normal = turn([0.0, 1.0, 0.0], angle);
    for h in &s.stabilised {
        let d = h.at.direction;
        let along = (d[0] * normal[0] + d[1] * normal[1] + d[2] * normal[2]).abs();
        assert!((along - 1.0).abs() < 1e-12, "{d:?}");
    }
    let u = solved(
        &upright,
        &nodal(top_loads(&upright, 0.0), 0.0),
        &first_order(true),
    );
    // A braced equivalent: a tie across the plane at every free joint.
    let mut braced = m.clone();
    let free: Vec<usize> = (0..m.nodes.len())
        .filter(|&n| !m.nodes[n].fixed[0])
        .collect();
    for n in free {
        let p = m.nodes[n].point;
        let q = [p[0] + normal[0], p[1] + normal[1], p[2] + normal[2]];
        let id = format!("tie{n}");
        braced.nodes.push(Node::fixed(&id, q));
        let k = braced.nodes.len() - 1;
        braced
            .members
            .push(Member::new(&id, MemberKind::Truss, n, k, 0));
    }
    for nonlinear in [false, true] {
        let o = Options {
            nonlinear,
            ..first_order(true)
        };
        let s = solved(&m, &loads, &o);
        let b = solved(&braced, &loads, &o);
        assert!(b.stabilised.is_empty());
        let u = if nonlinear {
            solved(&upright, &nodal(top_loads(&upright, 0.0), 0.0), &o)
        } else {
            u.clone()
        };
        for k in 0..m.members.len() {
            let (a, b, c) = (s.members[k].axial, b.members[k].axial, u.members[k].axial);
            assert!((a - b).abs() < 1e-6 * 1e4, "{k}: {a} {b}");
            assert!((a - c).abs() < 1e-6 * 1e4, "{k}: {a} {c}");
        }
    }
    // Under its own weight the joints' share across the plane (sin 10° of
    // half their members' weight) goes into the stabilisation: reported,
    // well below the limit.
    let w = solved(&m, &nodal(Vec::new(), 1.0), &first_order(true));
    let largest = w
        .stabilised
        .iter()
        .map(|h| h.force.abs())
        .fold(0.0, f64::max);
    assert!(largest > 1.0 && largest < 0.05 * m.weight(), "{largest}");
    assert!(
        stabilisation_note(&m, &w)
            .unwrap()
            .contains("% of the reactions")
    );
}

/// A load across the plane of a face joint goes nowhere: refused, naming
/// the joint.
#[test]
fn a_load_no_member_holds_is_refused() {
    let m = face(0.0);
    let mut loads = top_loads(&m, 0.0);
    let m2 = m.nodes.iter().position(|n| n.id == "M2").unwrap();
    loads.push(force(m2, [0.0, 2e3, 0.0]));
    let e = solve(
        &m,
        &nodal(loads, 0.0),
        &first_order(true),
        &default_budget(),
    )
    .unwrap_err();
    match &e {
        FeError::Held {
            joints, node, dof, ..
        } => {
            assert_eq!(joints, &vec!["M2".to_string()]);
            assert_eq!((node.as_str(), dof.as_str()), ("M2", "y"));
        }
        other => panic!("{other}"),
    }
    assert!(e.to_string().contains("no member holds"), "{e}");
}

/// A vertical frame column, pinned at its foot and stayed by two
/// trusses at its top: only its torsion holds its rotation about z, and
/// nothing holds the torsion — today "free to move in rz". Stabilised,
/// it solves as with rz held at the foot (the same equations).
#[test]
fn a_frame_free_to_spin_is_held_about_its_axis() {
    let mut m = Model {
        sections: vec![Section::elastic("s", E, 2e-3, 4e-6)],
        ..Model::default()
    };
    m.nodes = vec![
        Node::pinned("foot", [0.0, 0.0, 0.0]),
        Node::free("top", [0.0, 0.0, 6.0]),
        Node::pinned("g1", [4.0, 0.0, 0.0]),
        Node::pinned("g2", [0.0, 4.0, 0.0]),
    ];
    m.members = vec![
        Member::new("column", MemberKind::Frame, 0, 1, 0),
        Member::new("stay1", MemberKind::Truss, 1, 2, 0),
        Member::new("stay2", MemberKind::Truss, 1, 3, 0),
    ];
    let loads = nodal(vec![force(1, [3e3, 1e3, -5e3])], 0.0);
    let e = solve(&m, &loads, &first_order(false), &default_budget()).unwrap_err();
    assert!(
        matches!(&e, FeError::Mechanism { dof, .. } if *dof == "rz"),
        "{e}"
    );
    let s = solved(&m, &loads, &first_order(true));
    assert_eq!(s.stabilised.len(), 1);
    let h = &s.stabilised[0];
    assert_eq!((h.at.node, h.at.rotation, h.at.axis), (0, true, Some(2)));
    assert_eq!(h.at.dof_name(), "rz");
    let mut held = m.clone();
    held.nodes[0].fixed[5] = true;
    let r = solved(&held, &loads, &first_order(false));
    assert_eq!(s.displacements, r.displacements);
    assert_eq!(
        s.members.iter().map(|f| f.end_forces).collect::<Vec<_>>(),
        r.members.iter().map(|f| f.end_forces).collect::<Vec<_>>()
    );
    // A skew column spins about its own axis: a rotational spring there.
    let mut skew = m.clone();
    skew.nodes[1].point = [1.0, 0.5, 6.0];
    let s = solved(&skew, &loads, &first_order(true));
    assert_eq!(s.stabilised.len(), 1);
    assert!(s.stabilised[0].at.axis.is_none() && s.stabilised[0].at.rotation);
    assert!(s.stabilised[0].force.abs() < 1e-6);
}

/// What is no joint's own freedom stays a mechanism, naming the joints
/// that move: a frame of four bars without a diagonal sways (its top
/// joints), a triangle without supports floats (all of them).
#[test]
fn true_mechanisms_are_still_refused() {
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
    m.members = vec![
        Member::new("AD", MemberKind::Truss, 0, 3, 0),
        Member::new("BC", MemberKind::Truss, 1, 2, 0),
        Member::new("CD", MemberKind::Truss, 2, 3, 0),
    ];
    let loads = nodal(vec![force(3, [1e3, 0.0, 0.0])], 0.0);
    let e = solve(&m, &loads, &first_order(true), &default_budget()).unwrap_err();
    match &e {
        FeError::Mechanism { joints, moving, .. } => {
            let mut j = joints.clone();
            j.sort();
            assert_eq!((j, *moving), (vec!["C".to_string(), "D".to_string()], 2));
        }
        other => panic!("{other}"),
    }
    assert!(e.to_string().contains("with 2 joints: "), "{e}");
    // No supports at all.
    let mut t = Model {
        sections: m.sections.clone(),
        ..Model::default()
    };
    t.nodes = vec![
        Node::free("a", [0.0, 0.0, 0.0]),
        Node::free("b", [2.0, 0.0, 0.0]),
        Node::free("c", [1.0, 0.0, 1.5]),
    ];
    t.members = vec![
        Member::new("ab", MemberKind::Truss, 0, 1, 0),
        Member::new("bc", MemberKind::Truss, 1, 2, 0),
        Member::new("ca", MemberKind::Truss, 2, 0, 0),
    ];
    let e = solve(
        &t,
        &nodal(vec![force(2, [0.0, 0.0, -1e3])], 0.0),
        &first_order(true),
        &default_budget(),
    )
    .unwrap_err();
    assert!(
        matches!(&e, FeError::Mechanism { moving, .. } if *moving == 3),
        "{e}"
    );
}

/// A model that solves as it is gives the same solution, bit for bit,
/// with or without stabilisation (and lists none).
#[test]
fn a_stable_model_is_untouched() {
    let mut m = face(0.0);
    for n in &mut m.nodes {
        n.fixed[1] = true;
    }
    for nonlinear in [false, true] {
        let loads = nodal(top_loads(&m, 0.0), 1.0);
        let o = Options {
            nonlinear,
            ..first_order(true)
        };
        let with = solved(&m, &loads, &o);
        let without = solved(
            &m,
            &loads,
            &Options {
                stabilise: false,
                ..o
            },
        );
        assert!(with.stabilised.is_empty());
        assert_eq!(with, without);
    }
}

/// A tripod (three legs from feet on a 3 m circle to an apex 10 m up)
/// with one leg cut at its middle by a joint `M` with nothing else: `M`
/// has no stiffness across the leg's line (two directions). It is held
/// relative to the leg's ends (half each), so it follows the leg: with
/// P-Delta as in first order the tripod solves as the one with the leg
/// whole (the halves' geometric stiffness, with `M` on the line, is the
/// whole leg's), the holds take nothing, and the reactions balance the
/// loads. Held to the ground instead, `M` would pull the compressed leg
/// with its geometric stiffness N/(L/2) times its ends' sway.
#[test]
fn a_joint_on_a_line_follows_its_ends() {
    let section = Section::elastic("s", E, 1.2e-3, 0.0);
    let feet = [[3.0, 0.0, 0.0], [-1.5, 2.598, 0.0], [-1.5, -2.598, 0.0]];
    let apex = [0.0, 0.0, 10.0];
    let mut whole = Model {
        sections: vec![section],
        ..Model::default()
    };
    for (k, f) in feet.iter().enumerate() {
        whole.nodes.push(Node::pinned(&format!("F{k}"), *f));
    }
    whole.nodes.push(Node::free("A", apex));
    for k in 0..3 {
        whole
            .members
            .push(Member::new(&format!("leg{k}"), MemberKind::Truss, k, 3, 0));
    }
    let mut cut = whole.clone();
    let mid = [0, 1, 2].map(|c| (feet[0][c] + apex[c]) / 2.0);
    cut.nodes.push(Node::free("M", mid));
    cut.members[0] = Member::new("leg0a", MemberKind::Truss, 0, 4, 0);
    cut.members
        .push(Member::new("leg0b", MemberKind::Truss, 4, 3, 0));
    let load = [20e3, 5e3, -200e3];
    let size = load.iter().map(|v| v * v).sum::<f64>().sqrt();
    let loads = nodal(vec![force(3, load)], 0.0);
    let mut apex_moves = Vec::new();
    for nonlinear in [false, true] {
        let o = Options {
            nonlinear,
            ..first_order(true)
        };
        let w = solved(&whole, &loads, &o);
        let c = solved(&cut, &loads, &o);
        assert!(w.stabilised.is_empty());
        assert_eq!(c.stabilised.len(), 2);
        for h in &c.stabilised {
            assert_eq!(h.at.node, 4);
            assert_eq!(h.at.relative, vec![(0, 0.5), (3, 0.5)]);
            assert!(h.force.abs() < 1e-9 * size, "{nonlinear}: {}", h.force);
        }
        for d in 0..3 {
            let (a, b) = (w.displacements[3][d], c.displacements[3][d]);
            assert!((a - b).abs() < 1e-9 * a.abs().max(1e-6), "{d}: {a} {b}");
            // M on the line between its ends.
            let m = (c.displacements[0][d] + c.displacements[3][d]) / 2.0;
            assert!((c.displacements[4][d] - m).abs() < 1e-12, "{d}");
        }
        for (k, n) in [(0, 0), (0, 3), (1, 1), (2, 2)] {
            let (a, b) = (w.members[k].axial, c.members[n].axial);
            assert!((a - b).abs() < 1e-9 * size, "{k}: {a} {b}");
        }
        let sum: [f64; 3] = [0, 1, 2].map(|k| c.reactions.iter().map(|r| r.force[k]).sum());
        for k in 0..3 {
            assert!((sum[k] + load[k]).abs() < 1e-6 * size, "{sum:?}");
        }
        apex_moves.push(c.displacements[3][0]);
    }
    // P-Delta moves the apex further than first order.
    assert!(
        apex_moves[1].abs() > apex_moves[0].abs() * 1.001,
        "{apex_moves:?}"
    );
    // A small load on M across the leg (first order): the hold reports it
    // and passes it to the leg's ends, half each, as the whole leg would
    // carry it — the reactions balance all the loads.
    let across = [0.0, 100.0, 0.0];
    let mut with_m = loads.clone();
    with_m.nodal.push(force(4, across));
    let c = solved(&cut, &with_m, &first_order(true));
    let held: f64 = c
        .stabilised
        .iter()
        .map(|h| h.force * h.force)
        .sum::<f64>()
        .sqrt();
    assert!((held - 100.0).abs() < 1e-6, "{held}");
    let mut halves = loads.clone();
    halves.nodal.push(force(0, [0.0, 50.0, 0.0]));
    halves.nodal.push(force(3, [0.0, 50.0, 0.0]));
    let w = solved(&whole, &halves, &first_order(true));
    for (k, n) in [(0, 0), (1, 1), (2, 2)] {
        let (a, b) = (w.members[k].axial, c.members[n].axial);
        assert!((a - b).abs() < 1e-9 * size, "{k}: {a} {b}");
    }
    let sum: [f64; 3] = [0, 1, 2].map(|k| c.reactions.iter().map(|r| r.force[k]).sum());
    for k in 0..3 {
        assert!(
            (sum[k] + load[k] + across[k]).abs() < 1e-6 * size,
            "{sum:?}"
        );
    }
}
