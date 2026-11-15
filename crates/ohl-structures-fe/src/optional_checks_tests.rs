// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The optional checks of 2026-11-15 (owner), both off by default: a
//! redundant member's nominal force (`[analysis] redundant_force`, a
//! percentage of the compression in the member it braces) and the
//! climbing load (`[analysis] climbing_load`, flat members at mid-length,
//! alone). Invented models, numbers by hand.

use crate::check::{
    CLIMBING_INCLINATION_DEG, NOT_SAFE, REDUNDANT_FORCE, climbing_check, climbing_note,
    redundant_supports,
};
use crate::design::MemberClass;
use crate::model::{CapacityMethod, Section};
use crate::{
    Loads, Member, MemberKind, Model, NodalLoad, Node, Options, default_budget, member_usage, solve,
};

const E: f64 = 2.1e11;
const FY: f64 = 355e6;
/// The load on the column, N.
const P: f64 = 100e3;

/// A column A (0, 0, 0)–M (0, 0, 2)–B (0, 0, 4) of two truss pieces AM,
/// MB, pinned at A, B held across (x, y); M braced by the redundant
/// members R1 (to S1 at (2, 0, 2)) and R2 (to S2 at (0, 2, 2)), both
/// supports pinned; R3 from S1 to S3 (2, 2, 0) braces nothing a member
/// passes straight through. The simple check (Euler, squash): the
/// redundants (A 1 cm², I 1 cm⁴, 2 m) resist π² E I / L² = 5.18 kN in
/// compression, 35.5 kN in tension.
fn column(redundant_force: Option<f64>) -> Model {
    let mut leg = Section::elastic("leg", E, 4e-3, 4e-6);
    leg.yield_strength = Some(FY);
    let mut small = Section::elastic("small", E, 1e-4, 1e-8);
    small.yield_strength = Some(FY);
    let mut m = Model {
        sections: vec![leg, small],
        ..Model::default()
    };
    m.analysis.capacity = CapacityMethod::Simple;
    m.analysis.redundant_force = redundant_force;
    let mut top = Node::free("B", [0.0, 0.0, 4.0]);
    top.fixed = [true, true, false, false, false, false];
    m.nodes = vec![
        Node::pinned("A", [0.0, 0.0, 0.0]),
        Node::free("M", [0.0, 0.0, 2.0]),
        top,
        Node::pinned("S1", [2.0, 0.0, 2.0]),
        Node::pinned("S2", [0.0, 2.0, 2.0]),
        Node::pinned("S3", [2.0, 2.0, 0.0]),
    ];
    m.members = vec![
        Member::new("AM", MemberKind::Truss, 0, 1, 0),
        Member::new("MB", MemberKind::Truss, 1, 2, 0),
        Member::new("R1", MemberKind::Truss, 1, 3, 1),
        Member::new("R2", MemberKind::Truss, 1, 4, 1),
        Member::new("R3", MemberKind::Truss, 3, 5, 1),
    ];
    for k in [0, 1] {
        m.members[k].design.class = Some(MemberClass::Leg);
    }
    for k in [2, 3, 4] {
        m.members[k].design.class = Some(MemberClass::Redundant);
    }
    m
}

fn loaded(m: &Model, fz: f64) -> crate::Solution {
    let loads = Loads {
        nodal: vec![NodalLoad {
            node: 2,
            force: [0.0, 0.0, fz],
            moment: [0.0; 3],
        }],
        self_weight: 0.0,
        ..Default::default()
    };
    let options = Options {
        nonlinear: false,
        ..Options::default()
    };
    solve(m, &loads, &options, &default_budget()).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_redundant_member_takes_its_share_of_the_braced_compression() {
    // Off (the default): the redundants carry nothing and use 0.
    let off = column(None);
    let s = loaded(&off, -P);
    let u = member_usage(&off, &s, 1.0);
    assert!((s.members[0].axial + P).abs() < 1e-6 * P);
    assert_eq!(u[2].usage, Some(0.0));
    assert!(!u[2].governs.contains(REDUNDANT_FORCE), "{}", u[2].governs);
    // 2.5 %: R1 braces the column at M (AM and MB pass straight through
    // it): 2.5 kN, its compression capacity 5.18 kN — 48 %.
    let on = column(Some(0.025));
    let s = loaded(&on, -P);
    let supports = redundant_supports(&on, &s, 0.025);
    let r1 = supports[2].clone().unwrap().unwrap();
    assert_eq!((r1.member, r1.joint, r1.via_redundant), (0, 1, false));
    assert!((r1.force - P).abs() < 1e-6 * P, "{}", r1.force);
    assert!(supports[0].is_none() && supports[1].is_none());
    let u = member_usage(&on, &s, 1.0);
    let euler = std::f64::consts::PI.powi(2) * E * 1e-8 / 4.0;
    for k in [2, 3] {
        let x = u[k].usage.unwrap();
        assert!((x - 0.025 * P / euler).abs() < 1e-9, "{x}");
        assert!(
            u[k].governs.contains(
                "[redundant force: 2.5 % of 100.00 kN, the compression of AM at M (the member it \
                 braces): 2.50 kN — governs; own force 0 %]"
            ),
            "{}",
            u[k].governs
        );
        assert!(
            u[k].governs
                .starts_with("compression 2.5 kN of 5.2 kN (buckling)")
        );
    }
    // The legs are not redundant: unchanged.
    assert_eq!(u[0], member_usage(&off, &s, 1.0)[0]);
    // R3: nothing passes straight through S1 or S3 — said, not on the
    // safe side.
    assert!(supports[4].clone().unwrap().is_err());
    assert!(
        u[4].governs.contains(&format!(
            "{NOT_SAFE} redundant force (2.5 %) not checked: the member it braces not found"
        )),
        "{}",
        u[4].governs
    );
    assert!(crate::check::not_safe(&u[4].governs));
    // The column pulled: the braced member in tension, no nominal force.
    let s = loaded(&on, P);
    let u = member_usage(&on, &s, 1.0);
    assert_eq!(u[2].usage, Some(0.0));
    assert!(
        u[2].governs.contains("none in this case"),
        "{}",
        u[2].governs
    );
}

/// A redundant bracing a redundant (the hierarchy): R4 from the middle
/// of R1 (joint N, R1 split into R1a, R1b) to a support takes 2.5 % of
/// R1's design force — itself 2.5 % of the column's.
#[test]
fn a_sub_redundant_takes_its_share_of_the_redundant_it_braces() {
    let mut m = column(Some(0.025));
    // R1 split at N (1, 0, 2).
    m.nodes.push(Node::free("N", [1.0, 0.0, 2.0]));
    m.nodes.push(Node::pinned("S4", [1.0, 1.5, 2.0]));
    let (n, s4) = (m.nodes.len() - 2, m.nodes.len() - 1);
    m.members[2] = Member::new("R1a", MemberKind::Truss, 1, n, 1);
    m.members[2].design.class = Some(MemberClass::Redundant);
    let mut r1b = Member::new("R1b", MemberKind::Truss, n, 3, 1);
    r1b.design.class = Some(MemberClass::Redundant);
    m.members.push(r1b);
    let mut r4 = Member::new("R4", MemberKind::Truss, n, s4, 1);
    r4.design.class = Some(MemberClass::Redundant);
    m.members.push(r4);
    // N needs holding vertically: a support's spring.
    m.nodes[n].spring = [0.0, 0.0, 1e6, 0.0, 0.0, 0.0];
    let s = loaded(&m, -P);
    let supports = redundant_supports(&m, &s, 0.025);
    let r4 = supports[6].clone().unwrap().unwrap();
    assert!(r4.via_redundant, "{r4:?}");
    assert!((r4.force - 0.025 * P).abs() < 1e-6 * P, "{}", r4.force);
    let u = member_usage(&m, &s, 1.0);
    assert!(
        u[6].governs.contains("itself redundant: its design force"),
        "{}",
        u[6].governs
    );
}

/// A frame of members at several slopes: the flat ones (0°, 20°) are
/// checked for 1100 N at mid-length alone, the steep ones (40°, the
/// vertical) are not; numbers by hand.
#[test]
fn flat_members_carry_the_climbing_load_alone() {
    let mut s = Section::elastic("bar", E, 1e-4, 1e-8);
    s.yield_strength = Some(FY);
    s.wy = Some(2e-6);
    s.wz = Some(1e-6);
    s.density = 7850.0;
    let mut m = Model {
        sections: vec![s],
        ..Model::default()
    };
    let slope = |deg: f64| {
        let a = deg.to_radians();
        [2.0 * a.cos(), 0.0, 2.0 * a.sin()]
    };
    m.nodes = vec![
        Node::fixed("O", [0.0, 0.0, 0.0]),
        Node::free("H", slope(0.0)),
        Node::free("F", slope(20.0)),
        Node::free("S", slope(40.0)),
        Node::free("V", [0.0, 0.0, 2.0]),
    ];
    for (k, id) in ["h", "f", "s", "v"].iter().enumerate() {
        m.members
            .push(Member::new(id, MemberKind::Frame, 0, k + 1, 0));
    }
    // Off: nothing.
    assert!(climbing_check(&m).is_empty());
    assert!(climbing_note(&m, &[]).is_none());
    m.analysis.climbing_load = Some(1100.0);
    let c = climbing_check(&m);
    assert_eq!(
        c.iter().map(|c| c.member).collect::<Vec<_>>(),
        vec![0, 1],
        "{c:?}"
    );
    // M = P L / 4 + q L² / 8 on W_min = 1 cm³ (the smaller), f_y 355 MPa.
    let q = 7850.0 * 1e-4 * crate::model::G;
    let moment = 1100.0 * 2.0 / 4.0 + q * 4.0 / 8.0;
    let expected = moment / (1e-6 * FY);
    assert!((c[0].usage.unwrap() - expected).abs() < 1e-12, "{c:?}");
    assert_eq!(c[0].usage, c[1].usage);
    assert!(
        c[0].words.starts_with("1.10 kN at mid-length of 2.00 m"),
        "{}",
        c[0].words
    );
    let note = climbing_note(&m, &c).unwrap();
    assert!(
        note.starts_with(&format!(
            "climbing load (alone, not a load case; ASCE 10-97 commentary C2.2): 1.10 kN at \
             mid-length of each member within {CLIMBING_INCLINATION_DEG:.0}° of the horizontal"
        )),
        "{note}"
    );
    assert!(note.contains("2 member(s) checked"), "{note}");
    assert!(
        note.contains("2 OVER first yield: h 156 %, f 156 %"),
        "{note}"
    );
    assert!(
        note.contains("members steeper than 30° not checked"),
        "{note}"
    );
    // The load cases' usage does not change.
    let loads = Loads::default();
    let sol = solve(&m, &loads, &Options::default(), &default_budget()).unwrap();
    let mut without = m.clone();
    without.analysis.climbing_load = None;
    assert_eq!(
        member_usage(&m, &sol, 1.0),
        member_usage(&without, &sol, 1.0)
    );
}

#[test]
fn the_optional_checks_in_files() {
    let body = "[[material]]\nid = \"s\"\nelasticity = \"210 GPa\"\n\
                [[section]]\nid = \"a\"\nmaterial = \"s\"\narea = \"10 cm2\"\n\
                [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"pinned\"\n\
                [[node]]\nid = \"B\"\npoint = [0, 0, 5]\n\
                [[member]]\nid = \"t\"\nnodes = [\"A\", \"B\"]\nsection = \"a\"\n";
    let model = |analysis: &str| {
        Model::parse(&format!(
            "format = \"ohl-structure-model\"\nversion = 7\n{analysis}{body}"
        ))
    };
    let plain = model("").unwrap();
    assert_eq!(
        (plain.analysis.redundant_force, plain.analysis.climbing_load),
        (None, None)
    );
    let both = model("[analysis]\nredundant_force = 2.5\nclimbing_load = \"1100 N\"\n").unwrap();
    assert_eq!(both.analysis.redundant_force, Some(0.025));
    assert_eq!(both.analysis.climbing_load, Some(1100.0));
    let whole = model("[analysis]\nredundant_force = 2\nclimbing_load = \"1.5 kN\"\n").unwrap();
    assert_eq!(whole.analysis.redundant_force, Some(0.02));
    assert_eq!(whole.analysis.climbing_load, Some(1500.0));
    for bad in [
        "redundant_force = 0",
        "redundant_force = -1",
        "redundant_force = 150",
        "climbing_load = \"0 N\"",
        "climbing_load = \"-5 N\"",
    ] {
        let e = model(&format!("[analysis]\n{bad}\n")).unwrap_err();
        assert!(e.contains("analysis:"), "{bad}: {e}");
    }
    let s = Model::from_structure_type(&format!(
        "id = \"T\"\nname = \"t\"\nkind = \"lattice\"\n\
         [analysis]\nredundant_force = 1\nclimbing_load = \"1100 N\"\n{body}"
    ))
    .unwrap();
    assert_eq!(s.analysis.redundant_force, Some(0.01));
    assert_eq!(s.analysis.climbing_load, Some(1100.0));
    assert_eq!(crate::model::REDUNDANT_FORCE_MODEL_VERSION, 7);
    assert_eq!(crate::model::CLIMBING_LOAD_MODEL_VERSION, 7);
    assert_eq!(
        ohl_structures::structures::REDUNDANT_FORCE_VERSION,
        ohl_structures::structures::CLIMBING_LOAD_VERSION
    );
}
