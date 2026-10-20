// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tension-only members that bend (`bending = true`, a tower program's
//! tension-only beam) and members not design-checked (`check = false`, a
//! tower program's fictitious member), on invented models: a rod in two
//! halves carrying a load across it at its middle (against the fixed-end
//! beam's hand solution), the same rod as tension-only trusses (the load
//! across it carried only by a relative hold, flagged), an X-braced panel whose compressed
//! bending diagonal bows as a truss, and the file keys.

use crate::check::{NOT_CHECKED, worst};
use crate::{
    FeError, Loads, Member, MemberKind, Model, NodalLoad, Node, Options, Section, Solution,
    default_budget, member_usage, solve,
};

const E: f64 = 2.1e11;
/// Second moment of the rod, m⁴.
const I: f64 = 2e-6;
/// The pull along the rod, N.
const H: f64 = 10e3;
/// The load across the rod at its middle, N.
const P: f64 = 1e3;
/// The rod's length, m.
const L: f64 = 4.0;

fn force(node: usize, f: [f64; 3]) -> NodalLoad {
    NodalLoad {
        node,
        force: f,
        moment: [0.0; 3],
    }
}

fn first_order() -> Options {
    Options {
        nonlinear: false,
        ..Options::default()
    }
}

fn run(m: &Model, nodal: Vec<NodalLoad>) -> Result<Solution, FeError> {
    let loads = Loads {
        nodal,
        self_weight: 0.0,
    };
    solve(m, &loads, &first_order(), &default_budget())
}

/// A rod along x in two halves AM, MB through its middle M: A fixed, B
/// fixed but along x (so the pull H stretches the rod); `bending`: the
/// halves are tension-only members that bend, else tension-only trusses.
fn rod(bending: bool) -> Model {
    let mut m = Model {
        sections: vec![Section::elastic("s", E, 1e-3, I)],
        ..Model::default()
    };
    let mut b = Node::fixed("B", [L, 0.0, 0.0]);
    b.fixed[0] = false;
    m.nodes = vec![
        Node::fixed("A", [0.0, 0.0, 0.0]),
        Node::free("M", [L / 2.0, 0.0, 0.0]),
        b,
    ];
    for (id, i, j) in [("AM", 0, 1), ("MB", 1, 2)] {
        let mut x = Member::new(id, MemberKind::Tension, i, j, 0);
        x.bending = bending;
        m.members.push(x);
    }
    m
}

/// The pull H at B and P down at M. The halves stay taut (N = H) and
/// bend as one beam fixed at both ends with a load at its middle: M
/// moves down P·L³/(192·E·I) (first order: the pull does not stiffen
/// it), each end takes P/2 and the moment P·L/8.
#[test]
fn a_tension_only_beam_carries_a_load_across_it() {
    let m = rod(true);
    let s = run(&m, vec![force(2, [H, 0.0, 0.0]), force(1, [0.0, 0.0, -P])])
        .unwrap_or_else(|e| panic!("{e}"));
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * b.abs().max(1.0);
    let deflection = P * L.powi(3) / (192.0 * E * I);
    assert!(close(s.displacements[1][2], -deflection), "{s:?}");
    assert!(s.members.iter().all(|f| !f.slack && close(f.axial, H)));
    let a = s.reactions.iter().find(|r| r.node == 0).unwrap();
    assert!(close(a.force[2], P / 2.0), "{a:?}");
    assert!(close(a.moment[1].abs(), P * L / 8.0), "{a:?}");
    assert!(s.stabilised.is_empty());
    // The pull alone: a tie, M moves along it by H·(L/2)/EA.
    let t = run(&m, vec![force(2, [H, 0.0, 0.0])]).unwrap();
    assert!(close(t.displacements[1][0], H * L / 2.0 / (E * 1e-3)));
}

/// The same rod of tension-only trusses holds nothing across it by its
/// members: M lies on the straight line A–B, so the stabilisation holds it
/// relative to A and B and the load P (10 % of the loads, above the
/// limit) goes to them through that hold — not refused since the owner's
/// decision of 2026-10-19, but flagged ([`Solution::relative_over`]); the
/// halves carry no moment.
#[test]
fn a_tension_only_truss_rod_carries_it_only_by_a_relative_hold() {
    let m = rod(false);
    let s = run(&m, vec![force(2, [H, 0.0, 0.0]), force(1, [0.0, 0.0, -P])])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(s.relative_over.len(), 1);
    let h = &s.stabilised[s.relative_over[0]];
    assert!(h.at.node == 1 && !h.at.relative.is_empty());
    assert!((h.force.abs() - P).abs() <= 1e-9 * P);
    assert!(
        s.members
            .iter()
            .all(|f| f.end_forces[4] == 0.0 && f.end_forces[5] == 0.0)
    );
}

/// A panel 4 m wide and 3 m high in the x–z plane (feet A, B pinned, top
/// D, C; legs and top trusses), held across its plane at the top joints;
/// its diagonals AC, BD (5 m) tension-only members that bend, of a second
/// moment I_d = 1e-10 m⁴: buckling load N_cr = π²·E·I_d/L² = 8.29 N. H
/// along +x at D: BD is shortened beyond N_cr, bowed — a tension-only
/// truss carrying −N_cr, no bending, no moments; AC carries the rest of
/// the shear (method of joints with BD's compression c = N_cr, as in
/// structfe-011: AC = 5H/4 − c, DC = −H + 0.8c) and bends not at all.
#[test]
fn a_compressed_bending_diagonal_bows_as_a_truss() {
    let i_d = 1e-10;
    let mut m = Model {
        sections: vec![
            Section::elastic("s", E, 1e-3, I),
            Section::elastic("d", E, 1e-3, i_d),
        ],
        ..Model::default()
    };
    let mut c = Node::free("C", [4.0, 0.0, 3.0]);
    let mut d = Node::free("D", [0.0, 0.0, 3.0]);
    c.fixed[1] = true;
    d.fixed[1] = true;
    m.nodes = vec![
        Node::pinned("A", [0.0, 0.0, 0.0]),
        Node::pinned("B", [4.0, 0.0, 0.0]),
        c,
        d,
    ];
    m.members = vec![
        Member::new("AD", MemberKind::Truss, 0, 3, 0),
        Member::new("BC", MemberKind::Truss, 1, 2, 0),
        Member::new("DC", MemberKind::Truss, 3, 2, 0),
    ];
    for (id, i, j) in [("AC", 0, 2), ("BD", 1, 3)] {
        let mut x = Member::new(id, MemberKind::Tension, i, j, 1);
        x.bending = true;
        m.members.push(x);
    }
    let n_cr = std::f64::consts::PI.powi(2) * E * i_d / 25.0;
    let s = run(&m, vec![force(3, [H, 0.0, 0.0])]).unwrap_or_else(|e| panic!("{e}"));
    let (ac, bd) = (&s.members[3], &s.members[4]);
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * H;
    assert!(bd.bowed && close(bd.axial, -n_cr), "{s:?}");
    let bd_moments = [4, 5, 10, 11].map(|k| bd.end_forces[k]);
    assert_eq!(bd_moments, [0.0; 4]);
    assert!(close(ac.axial, 1.25 * H - n_cr), "{}", ac.axial);
    let moments = [4, 5, 10, 11].map(|k| ac.end_forces[k].abs());
    assert!(moments.iter().all(|x| *x <= 1e-6 * H), "{moments:?}");
    assert!(close(s.members[2].axial, -H + 0.8 * n_cr));
}

/// A member with `check = false` is solved, its forces reported, its
/// usage not: none, and the worst member is another.
#[test]
fn a_member_not_checked_has_no_usage() {
    let mut m = rod(true);
    m.sections[0].yield_strength = Some(355e6);
    m.sections[0].wy = Some(1e-5);
    m.sections[0].wz = Some(1e-5);
    m.members[0].check = false;
    let s = run(&m, vec![force(2, [H, 0.0, 0.0]), force(1, [0.0, 0.0, -P])]).unwrap();
    assert!(s.members[0].axial > 0.0);
    let u = member_usage(&m, &s, 1.0);
    assert_eq!((u[0].usage, u[0].governs.as_str()), (None, NOT_CHECKED));
    // MB as a frame: H/(A·fy) + (P·L/8)/(W·fy) at its ends.
    let expected = H / (1e-3 * 355e6) + P * L / 8.0 / (1e-5 * 355e6);
    let mb = u[1].usage.unwrap();
    assert!((mb - expected).abs() <= 1e-9, "{mb} {expected}");
    assert_eq!(worst(&u).unwrap().member, 1);
}

#[test]
fn bending_and_check_in_files() {
    let body = "[[material]]\nid = \"s\"\nelasticity = \"210 GPa\"\n\
                [[section]]\nid = \"a\"\nmaterial = \"s\"\narea = \"10 cm2\"\n\
                iy = \"100 cm4\"\niz = \"100 cm4\"\n\
                [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"fixed\"\n\
                [[node]]\nid = \"B\"\npoint = [0, 0, 5]\n\
                [[member]]\nid = \"t\"\nnodes = [\"A\", \"B\"]\nsection = \"a\"\n";
    let m = Model::parse(&format!(
        "format = \"ohl-structure-model\"\nversion = 3\n{body}\
         kind = \"tension\"\nbending = true\ncheck = false\n"
    ))
    .unwrap();
    let t = &m.members[0];
    assert!(t.bending && !t.check && t.is_frame());
    assert_eq!(t.kind, MemberKind::Tension);
    // Defaults: no bending, checked.
    let plain = Model::parse(&format!("{body}kind = \"tension\"\n")).unwrap();
    assert!(!plain.members[0].bending && plain.members[0].check && !plain.members[0].is_frame());
    // In a structure type file too (version 6).
    let s = Model::from_structure_type(&format!(
        "id = \"T\"\nname = \"t\"\nkind = \"lattice\"\nversion = 6\n{body}\
         kind = \"tension\"\nbending = true\ncheck = false\n"
    ))
    .unwrap();
    assert!(s.members[0].bending && !s.members[0].check);
    // Bending is for tension-only members.
    for kind in ["frame", "truss", "cable"] {
        let e = Model::parse(&format!("{body}kind = \"{kind}\"\nbending = true\n")).unwrap_err();
        assert!(e.contains("bending is for tension-only members"), "{e}");
    }
}
