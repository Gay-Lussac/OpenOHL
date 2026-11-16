// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Prescribed support displacements (measured foot movements, owner
//! 2026-11-16): none changes nothing (bit for bit); a settled support of a
//! continuous beam against the closed form; a settled foot of a
//! four-legged tower against an independent solve (the foot released and
//! pushed by the force that moves it as far); a rigid-body motion strains
//! nothing; a pole's tilt and its P-Delta; a spring's ground end; the
//! tension-only members' active set; the condensation's and the deflected
//! body's answer against the solve.

use super::*;
use crate::condense::{Attachment, Linearisation, condense_full, deflect_full};

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

fn moved(node: usize, displacement: [f64; 6]) -> Prescribed {
    Prescribed { node, displacement }
}

fn solved(m: &Model, loads: &Loads, options: &Options) -> Solution {
    solve(m, loads, options, &default_budget()).unwrap_or_else(|e| panic!("{e}"))
}

fn reaction(s: &Solution, node: usize) -> Reaction {
    *s.reactions
        .iter()
        .find(|r| r.node == node)
        .unwrap_or_else(|| panic!("no reaction at node {node}"))
}

/// A beam along x over three supports at 0, a and 2a (pinned, held about
/// x and z so it bends in the x–z plane only), a joint in the middle of
/// each span.
fn continuous_beam(a: f64) -> (Model, f64) {
    let s = Section::elastic("s", E, 1e-2, 3e-5);
    let i = s.iy;
    let mut m = Model {
        sections: vec![s],
        ..Model::default()
    };
    let support = |id: &str, x: f64| {
        let mut n = Node::pinned(id, [x, 0.0, 0.0]);
        n.fixed[3] = true;
        n.fixed[5] = true;
        n
    };
    m.nodes = vec![
        support("A", 0.0),
        Node::free("AB", [0.5 * a, 0.0, 0.0]),
        support("B", a),
        Node::free("BC", [1.5 * a, 0.0, 0.0]),
        support("C", 2.0 * a),
    ];
    // The middle joints held across and about x, z (a plane frame).
    for k in [1, 3] {
        m.nodes[k].fixed[1] = true;
        m.nodes[k].fixed[3] = true;
        m.nodes[k].fixed[5] = true;
    }
    m.members = (0..4)
        .map(|k| Member::new(&format!("m{k}"), MemberKind::Frame, k, k + 1, 0))
        .collect();
    (m, i)
}

/// Without a prescribed displacement (none, or all zero) every solve is
/// the same, to the last digit: first order, P-Delta, with tension-only
/// members.
#[test]
fn no_prescribed_displacement_changes_nothing() {
    let (m, _) = continuous_beam(5.0);
    let mut loaded = Loads {
        nodal: vec![load(1, [0.0, 0.0, -10e3]), load(3, [0.0, 0.0, -4e3])],
        self_weight: 1.0,
        ..Loads::default()
    };
    for options in [first_order(), Options::default()] {
        let plain = solved(&m, &loaded, &options);
        loaded.prescribed = vec![moved(2, [0.0; 6]), moved(0, [0.0; 6])];
        let zero = solved(&m, &loaded, &options);
        loaded.prescribed.clear();
        assert_eq!(plain, zero);
    }
    let (t, nodal) = braced_tower();
    let mut loads = Loads {
        nodal,
        ..Loads::default()
    };
    let plain = solved(&t, &loads, &Options::default());
    loads.prescribed = vec![moved(0, [0.0; 6])];
    assert_eq!(plain, solved(&t, &loads, &Options::default()));
}

/// The middle support of a two-span beam settled by δ: its reaction
/// 6 E I δ / a³ (the load at mid-length of a simply supported beam of
/// 2a that deflects it by δ: P (2a)³ / 48 E I), the moment over it
/// 3 E I δ / a², the outer reactions half of it the other way.
#[test]
fn a_settled_middle_support_matches_the_closed_form() {
    let a = 5.0;
    let delta = -0.01;
    let (m, i) = continuous_beam(a);
    let loads = Loads {
        prescribed: vec![moved(2, [0.0, 0.0, delta, 0.0, 0.0, 0.0])],
        ..Loads::default()
    };
    let s = solved(&m, &loads, &first_order());
    let r = 6.0 * E * i * delta.abs() / a.powi(3);
    // Held lower than the beam would sit, the support pulls it down.
    assert!(
        close(reaction(&s, 2).force[2], -r, 1e-9),
        "{:?}",
        s.reactions
    );
    assert!(close(reaction(&s, 0).force[2], 0.5 * r, 1e-9));
    assert!(close(reaction(&s, 4).force[2], 0.5 * r, 1e-9));
    // The support moved as given.
    assert_eq!(s.displacements[2][2], delta);
    // The moment over it, from the end forces of the member ending there.
    let moment = s.members[1].end_forces[9..12]
        .iter()
        .fold(0.0f64, |a, b| a.max(b.abs()));
    assert!(
        close(moment, 3.0 * E * i * delta.abs() / (a * a), 1e-9),
        "{moment}"
    );
    // The reactions balance (no load).
    let sum: f64 = s.reactions.iter().map(|r| r.force[2]).sum();
    assert!(sum.abs() < 1e-6 * r, "{sum}");
}

/// All supports moved alike: a rigid-body motion, no member strained.
#[test]
fn a_rigid_body_motion_strains_nothing() {
    let (m, _) = continuous_beam(5.0);
    let d = [0.012, 0.0, -0.03, 0.0, 0.0, 0.0];
    let loads = Loads {
        prescribed: (0..3).map(|k| moved(2 * k, d)).collect(),
        ..Loads::default()
    };
    let s = solved(&m, &loads, &first_order());
    for f in &s.members {
        assert!(f.end_forces.iter().all(|x| x.abs() < 1e-3), "{f:?}");
    }
    for u in &s.displacements {
        assert!((u[0] - d[0]).abs() < 1e-12 && (u[2] - d[2]).abs() < 1e-12);
    }
}

/// A square tower of four legs (pinned feet), braced on each face and at
/// its top: statically indeterminate on its feet. The loads: a pull at
/// the top across.
fn braced_tower() -> (Model, Vec<NodalLoad>) {
    let (b, t, h) = (6.0, 2.0, 20.0);
    let mut m = Model {
        sections: vec![
            Section::elastic("leg", E, 4e-3, 4e-6),
            Section::elastic("brace", E, 1e-3, 1e-7),
        ],
        ..Model::default()
    };
    let corners = |w: f64| [[-w, -w], [w, -w], [w, w], [-w, w]];
    for (k, c) in corners(b / 2.0).iter().enumerate() {
        m.nodes
            .push(Node::pinned(&format!("F{k}"), [c[0], c[1], 0.0]));
    }
    for (k, c) in corners(t / 2.0).iter().enumerate() {
        m.nodes.push(Node::free(&format!("T{k}"), [c[0], c[1], h]));
    }
    let mut add = |id: String, kind: MemberKind, i: usize, j: usize, s: usize| {
        m.members.push(Member::new(&id, kind, i, j, s));
    };
    for k in 0..4 {
        let n = (k + 1) % 4;
        add(format!("L{k}"), MemberKind::Truss, k, 4 + k, 0);
        add(format!("D{k}a"), MemberKind::Truss, k, 4 + n, 1);
        add(format!("D{k}b"), MemberKind::Truss, n, 4 + k, 1);
        add(format!("H{k}"), MemberKind::Truss, 4 + k, 4 + n, 1);
    }
    add("X".into(), MemberKind::Truss, 4, 6, 1);
    (m, vec![load(5, [5e3, 0.0, -2e3])])
}

/// One foot of the tower settled by δ: the members' forces and the
/// reactions are those of an independent solve — the foot released
/// vertically and pushed by the force that moves it by δ (from a solve
/// under a unit force there, the structure linear), the other loads on.
#[test]
fn a_settled_foot_of_a_tower_matches_an_independent_solve() {
    let (m, nodal) = braced_tower();
    let delta = -0.015;
    let loads = Loads {
        nodal: nodal.clone(),
        prescribed: vec![moved(2, [0.0, 0.0, delta, 0.0, 0.0, 0.0])],
        ..Loads::default()
    };
    let s = solved(&m, &loads, &first_order());
    // Independent: foot 2 free in z.
    let mut released = m.clone();
    released.nodes[2].fixed[2] = false;
    let with = |extra: f64, own: bool| {
        let mut l = Loads {
            nodal: if own { nodal.clone() } else { Vec::new() },
            ..Loads::default()
        };
        l.nodal.push(load(2, [0.0, 0.0, extra]));
        solved(&released, &l, &first_order())
    };
    let loaded = with(0.0, true);
    let unit = with(1.0, false);
    let force = (delta - loaded.displacements[2][2]) / unit.displacements[2][2];
    let independent = with(force, true);
    assert!(close(independent.displacements[2][2], delta, 1e-9));
    for (a, b) in s.members.iter().zip(&independent.members) {
        assert!(
            (a.axial - b.axial).abs() <= 1e-6 * force.abs().max(1.0),
            "{} {}",
            a.axial,
            b.axial
        );
    }
    // The settled foot's reaction is the pushing force (with what the
    // released foot took of the loads: nothing in z there).
    assert!(close(reaction(&s, 2).force[2], force, 1e-6));
    // The settlement strains the tower: forces differ from its loads'
    // alone, by much more than round-off.
    let alone = solved(
        &m,
        &Loads {
            nodal,
            ..Loads::default()
        },
        &first_order(),
    );
    let change = s
        .members
        .iter()
        .zip(&alone.members)
        .map(|(a, b)| (a.axial - b.axial).abs())
        .fold(0.0, f64::max);
    assert!(change > 1e3, "{change}");
}

/// A pole (a cantilever fixed at its base) tilted by θ about y: first
/// order a rigid-body motion — the top moves θ·H, no force; with P-Delta
/// a weight at the top leans with it: the base moment is the weight times
/// the top's lean (the weight's arm), the larger by the bending.
#[test]
fn a_tilted_pole_leans_and_its_weight_bends_it() {
    let h = 12.0;
    let s0 = Section::elastic("s", E, 5e-3, 4e-5);
    let mut m = Model {
        sections: vec![s0],
        ..Model::default()
    };
    m.nodes = vec![
        Node::fixed("base", [0.0, 0.0, 0.0]),
        Node::free("mid", [0.0, 0.0, 0.5 * h]),
        Node::free("top", [0.0, 0.0, h]),
    ];
    m.members = vec![
        Member::new("a", MemberKind::Frame, 0, 1, 0),
        Member::new("b", MemberKind::Frame, 1, 2, 0),
    ];
    let theta = 0.01;
    let tilt = vec![moved(0, [0.0, 0.0, 0.0, 0.0, theta, 0.0])];
    let first = solved(
        &m,
        &Loads {
            prescribed: tilt.clone(),
            ..Loads::default()
        },
        &first_order(),
    );
    assert!(close(first.displacements[2][0], theta * h, 1e-9));
    assert!(
        first
            .members
            .iter()
            .all(|f| f.end_forces.iter().all(|x| x.abs() < 1e-3))
    );
    let p = 20e3;
    let second = solved(
        &m,
        &Loads {
            nodal: vec![load(2, [0.0, 0.0, -p])],
            prescribed: tilt,
            ..Loads::default()
        },
        &Options::default(),
    );
    let lean = second.displacements[2][0];
    assert!(lean > theta * h, "{lean}");
    let base = reaction(&second, 0).moment[1].abs();
    assert!(close(base, p * lean, 1e-3), "{base} {}", p * lean);
}

/// A spring's ground end moved by g: a bar of stiffness EA/L in line with
/// the spring k takes u = k g / (k + EA/L); the spring's force is the
/// bar's.
#[test]
fn a_springs_ground_end_moves() {
    let (l, area, k, g) = (3.0, 1e-4, 2e6, 0.02);
    let mut m = Model {
        sections: vec![Section::elastic("s", E, area, 1e-8)],
        ..Model::default()
    };
    let mut top = Node::free("top", [0.0, 0.0, l]);
    top.fixed[0] = true;
    top.fixed[1] = true;
    top.spring[2] = k;
    m.nodes = vec![Node::pinned("base", [0.0, 0.0, 0.0]), top];
    m.members = vec![Member::new("bar", MemberKind::Truss, 0, 1, 0)];
    let s = solved(
        &m,
        &Loads {
            prescribed: vec![moved(1, [0.0, 0.0, g, 0.0, 0.0, 0.0])],
            ..Loads::default()
        },
        &first_order(),
    );
    let ea_l = E * area / l;
    let u = k * g / (k + ea_l);
    assert!(close(s.displacements[1][2], u, 1e-9));
    assert!(close(s.members[0].axial, ea_l * u, 1e-9));
    assert!(close(reaction(&s, 1).force[2], -k * (u - g), 1e-9));
}

/// A free direction is not a support: refused, named.
#[test]
fn a_free_direction_cannot_be_moved() {
    let (m, _) = continuous_beam(5.0);
    let e = solve(
        &m,
        &Loads {
            prescribed: vec![moved(1, [0.0, 0.0, -0.01, 0.0, 0.0, 0.0])],
            ..Loads::default()
        },
        &first_order(),
        &default_budget(),
    )
    .unwrap_err();
    assert!(e.to_string().contains("joint AB in z"), "{e}");
}

/// A square panel braced by two tension-only diagonals, its right foot
/// settled: the diagonal it shortens goes slack (its buckling load 0
/// without a second moment), the other carries what the settlement puts
/// on the panel — the same as the panel with the slack one left out.
#[test]
fn a_settlement_slackens_a_tension_only_diagonal() {
    let w = 4.0;
    let mut m = Model {
        sections: vec![
            Section::elastic("frame", E, 4e-3, 2e-5),
            Section::elastic("rod", E, 3e-4, 0.0),
        ],
        ..Model::default()
    };
    // Fixed feet: a settled foot racks the panel (pinned ones would let
    // it turn as a rigid body about the other).
    m.nodes = vec![
        Node::fixed("A", [0.0, 0.0, 0.0]),
        Node::fixed("B", [w, 0.0, 0.0]),
        Node::free("C", [w, 0.0, w]),
        Node::free("D", [0.0, 0.0, w]),
    ];
    for k in [2, 3] {
        m.nodes[k].fixed[1] = true;
        m.nodes[k].fixed[3] = true;
        m.nodes[k].fixed[5] = true;
    }
    m.members = vec![
        Member::new("AD", MemberKind::Frame, 0, 3, 0),
        Member::new("BC", MemberKind::Frame, 1, 2, 0),
        Member::new("DC", MemberKind::Frame, 3, 2, 0),
        Member::new("AC", MemberKind::Tension, 0, 2, 1),
        Member::new("BD", MemberKind::Tension, 1, 3, 1),
    ];
    let loads = Loads {
        prescribed: vec![moved(1, [0.0, 0.0, -0.01, 0.0, 0.0, 0.0])],
        ..Loads::default()
    };
    let s = solved(&m, &loads, &first_order());
    // B down: C goes down with it, A–C is shortened (slack); B moves
    // away from D, B–D is stretched.
    assert!(s.members[3].slack, "{:?}", s.members[3]);
    assert!(s.members[4].axial > 0.0 && !s.members[4].slack);
    let mut without = m.clone();
    without.members.remove(3);
    let alone = solved(&without, &loads, &first_order());
    assert!(close(s.members[4].axial, alone.members[3].axial, 1e-9));
}

/// The condensation's foot movements and the deflected body on its moved
/// feet are the solve's (the elastic structure, first order).
#[test]
fn the_condensation_and_the_body_take_the_feet_as_the_solve() {
    let (mut m, nodal) = braced_tower();
    m.nodes[5].attachment = Some("P".into());
    m.joint_points.push(crate::model::JointPoint {
        label: "P".into(),
        node: 5,
        offset: [0.0; 3],
    });
    let feet = vec![
        moved(1, [0.004, 0.0, -0.01, 0.0, 0.0, 0.0]),
        moved(3, [0.0, 0.0, 0.006, 0.0, 0.0, 0.0]),
    ];
    let s = solved(
        &m,
        &Loads {
            nodal: nodal.clone(),
            prescribed: feet.clone(),
            ..Loads::default()
        },
        &first_order(),
    );
    let c = condense_full(
        &m,
        &[Attachment::of_type("P")],
        &Linearisation::Unloaded,
        &[],
        &feet,
        &default_budget(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let alone = solved(
        &m,
        &Loads {
            prescribed: feet.clone(),
            ..Loads::default()
        },
        &first_order(),
    );
    let at = c.feet_at_points().expect("feet given");
    for k in 0..3 {
        assert!(
            (at[0][k] - alone.displacements[5][k]).abs() < 1e-12,
            "{at:?}"
        );
    }
    let body = deflect_full(
        &m,
        &Linearisation::Unloaded,
        &nodal,
        &feet,
        &default_budget(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    for (a, b) in body.iter().zip(&s.displacements) {
        for k in 0..3 {
            assert!((a[k] - b[k]).abs() < 1e-12, "{a:?} {b:?}");
        }
    }
    // None given: none read.
    let plain = condense_full(
        &m,
        &[Attachment::of_type("P")],
        &Linearisation::Unloaded,
        &[],
        &[],
        &default_budget(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(plain.feet.is_empty());
    assert_eq!(plain.flexibility, c.flexibility);
}

/// A pole's base tilted: every point of its condensation moves by the
/// rigid rotation (θ × r), its axis drawn leaning.
#[test]
fn a_poles_base_tilt_moves_its_points_rigidly() {
    let pole = crate::Pole::parse(
        "format = \"ohl-pole\"\nversion = 1\nid = \"P\"\nmaterial = \"steel\"\n\
         length = \"17 m\"\nembedment = \"2 m\"\ntop_diameter = \"200 mm\"\n\
         ground_diameter = \"400 mm\"\nwall = \"6 mm\"\nmoment_capacity = \"300 kN m\"\n\
         elasticity = \"210 GPa\"\n",
    );
    let pole = match pole {
        Ok(p) => p,
        Err(e) => panic!("{e}"),
    };
    let theta = 0.004;
    let base = [0.0, 0.0, -0.02, -theta, 0.0, 0.0];
    let points = vec![("top".to_owned(), [1.5, 0.0, 14.0])];
    let c = crate::condense::condense_pole_full(&pole, &points, None, &[], base, &default_budget())
        .unwrap_or_else(|e| panic!("{e}"));
    let at = c.feet_at_points().expect("given")[0];
    // −θ about x: a point at height z moves to −y by… θ × r with
    // θ = (−θ, 0, 0), r = (1.5, 0, 14): (0·14 − 0·0, 0·1.5 − (−θ)·14, −θ·0 − 0) …
    let r = [1.5, 0.0, 14.0];
    let w = [-theta, 0.0, 0.0];
    let rigid = [
        w[1] * r[2] - w[2] * r[1],
        w[2] * r[0] - w[0] * r[2],
        w[0] * r[1] - w[1] * r[0] - 0.02,
    ];
    for k in 0..3 {
        assert!((at[k] - rigid[k]).abs() < 1e-9, "{at:?} {rigid:?}");
    }
    let axis = crate::condense::deflect_pole_full(
        &pole,
        None,
        &[],
        &crate::condense::PoleBody::default(),
        base,
        &default_budget(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let (h, top) = *axis.last().expect("nodes");
    assert!((top[1] - theta * h).abs() < 1e-9, "{top:?}");
}

/// The pole check of a tilted pole: first order, a weight P at the top
/// leaning by θ·H bends it by P·θ·H at the ground (with a horizontal pull
/// as before); with P-Delta the moment grows more; no lean: as before.
#[test]
fn the_pole_check_takes_the_lean() {
    let pole = crate::Pole::parse(
        "format = \"ohl-pole\"\nversion = 1\nid = \"P\"\nmaterial = \"steel\"\n\
         length = \"17 m\"\nembedment = \"2 m\"\ntop_diameter = \"200 mm\"\n\
         ground_diameter = \"400 mm\"\nwall = \"6 mm\"\nmoment_capacity = \"300 kN m\"\n\
         elasticity = \"210 GPa\"\n",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let h = pole.height();
    let (p, pull, theta) = (30e3, 2e3, 0.01);
    let loads = [crate::PoleLoad {
        point: [0.0, 0.0, h],
        force: [pull, 0.0, -p],
    }];
    let b = default_budget();
    let upright = crate::check_pole_factored(&pole, &loads, [0.0; 2], 1.0, 1.0, false, &b)
        .unwrap_or_else(|e| panic!("{e}"));
    let same = crate::check_pole_leaning(&pole, &loads, [0.0; 2], 1.0, 1.0, [0.0; 2], false, &b)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(upright, same);
    let leaning =
        crate::check_pole_leaning(&pole, &loads, [0.0; 2], 1.0, 1.0, [theta, 0.0], false, &b)
            .unwrap_or_else(|e| panic!("{e}"));
    assert!(close(leaning.ground_moment, pull * h + p * theta * h, 1e-9));
    let second =
        crate::check_pole_leaning(&pole, &loads, [0.0; 2], 1.0, 1.0, [theta, 0.0], true, &b)
            .unwrap_or_else(|e| panic!("{e}"));
    let plain = crate::check_pole_factored(&pole, &loads, [0.0; 2], 1.0, 1.0, true, &b)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(second.amplification > 1.0, "{}", second.amplification);
    assert!(second.moment > leaning.moment && second.moment > plain.moment);
}
