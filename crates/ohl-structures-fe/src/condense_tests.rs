// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks of the condensation of a structure to its attachment points
//! ([`crate::condense`]): beam theory in closed form, the full solve,
//! the tangent at a reference state against finite differences, the
//! refusals.

use ohl_core::budget::Budget;

use crate::condense::{
    Attachment, CondenseKey, Linearisation, Link, Rigid, condense, condense_pole,
    place_displacement,
};
use crate::model::{JointPoint, Member, MemberKind, Model, NodalLoad, Node, Section};
use crate::pole::{Pole, PoleCode, PoleLoad, PoleMaterial};
use crate::solve::{Loads, Options, default_budget, solve};

const E: f64 = 2.1e11;

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs().max(1e-30)
}

/// A vertical frame column fixed at its foot, nodes at `heights` (m,
/// rising), one section (area `a`, second moment `i` both ways).
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
    m
}

fn point(m: &mut Model, label: &str, node: usize, offset: [f64; 3]) {
    m.joint_points.push(JointPoint {
        label: label.into(),
        node,
        offset,
    });
}

fn points(labels: &[&str]) -> Vec<Attachment> {
    labels.iter().map(|l| Attachment::of_type(l)).collect()
}

/// A cantilever column with one attachment point at its top: the
/// condensed stiffness is 3EI/L³ across (both ways) and EA/L along it,
/// with no coupling between the three (beam theory, exact for a point
/// load on cubic elements).
#[test]
fn a_cantilever_condenses_to_its_beam_stiffness() {
    let (l, a, i) = (12.0, 4e-3, 3e-5);
    let mut m = column(&[l], a, i);
    point(&mut m, "A1", 1, [0.0; 3]);
    let c = condense(
        &m,
        &points(&["A1"]),
        &Linearisation::Unloaded,
        &default_budget(),
    )
    .unwrap();
    let k = c.stiffness.as_ref().expect("positive definite");
    let across = 3.0 * E * i / l.powi(3);
    let along = E * a / l;
    for (d, expected) in [(0, across), (1, across), (2, along)] {
        assert!(
            close(k[(d, d)], expected, 1e-9),
            "K[{d}][{d}] = {} against {expected}",
            k[(d, d)]
        );
        assert!(close(c.flexibility[(d, d)], 1.0 / expected, 1e-9));
    }
    for (r, s) in [(0, 1), (0, 2), (1, 2)] {
        assert!(k[(r, s)].abs() < 1e-9 * across, "{k}");
    }
    assert!(c.notes.is_empty(), "{:?}", c.notes);
}

/// Two points on a cantilever at heights a ≤ b: the coupling across is
/// C_ab = a²(3b − a)/(6EI) (beam theory), the same both ways (Maxwell),
/// and a point at an offset e from the axis turns with the column's
/// twist: C = b³/(3EI) + e²·b/(GJ) for a force across the offset.
#[test]
fn two_points_couple_as_beam_theory_says() {
    let (a, b, area, i) = (7.0, 12.0, 4e-3, 3e-5);
    let mut m = column(&[a, b], area, i);
    point(&mut m, "low", 1, [0.0; 3]);
    point(&mut m, "high", 2, [0.0; 3]);
    let e = 1.5;
    point(&mut m, "arm", 2, [e, 0.0, 0.0]);
    let c = condense(
        &m,
        &points(&["low", "high", "arm"]),
        &Linearisation::Unloaded,
        &default_budget(),
    )
    .unwrap();
    assert_eq!(c.places.len(), 3, "{:?}", c.places);
    let ei = E * i;
    let coupling = a * a * (3.0 * b - a) / (6.0 * ei);
    let lh = c.flexibility_between("low", "high").unwrap();
    let hl = c.flexibility_between("high", "low").unwrap();
    for d in 0..2 {
        assert!(close(lh[(d, d)], coupling, 1e-9), "{lh}");
        assert!(close(hl[(d, d)], coupling, 1e-9), "{hl}");
    }
    let high = c.flexibility_between("high", "high").unwrap();
    assert!(close(high[(0, 0)], b.powi(3) / (3.0 * ei), 1e-9));
    // The arm: a force along y at x = e twists the column by F e b / GJ.
    let gj = (E / 2.6) * 2.0 * i;
    let arm = c.flexibility_between("arm", "arm").unwrap();
    assert!(
        close(arm[(1, 1)], b.powi(3) / (3.0 * ei) + e * e * b / gj, 1e-9),
        "{arm}"
    );
    // Symmetric (Maxwell–Betti) with the offsets' couplings.
    let f = &c.flexibility;
    assert!((f - f.transpose()).abs().max() < 1e-12 * f.abs().max());
    // Two points of one rigid link (high and arm on one node) cannot
    // move apart along their line: no stiffness, said.
    assert!(c.stiffness.is_none());
    assert!(
        c.notes.iter().any(|n| n.contains("no stiffness")),
        "{:?}",
        c.notes
    );
}

/// A portal frame with two points (one relinked to the other corner at
/// an offset, as a placed structure may): the condensed flexibility is
/// the full solve's displacements under unit loads at the points
/// (mapped by the same function as the structure check's loads),
/// symmetric and positive definite, and its stiffness is its inverse.
#[test]
fn a_portal_condenses_to_the_full_solve() {
    let mut m = Model {
        sections: vec![
            Section::elastic("col", E, 6e-3, 8e-5),
            Section::elastic("beam", E, 5e-3, 5e-5),
        ],
        ..Model::default()
    };
    m.nodes = vec![
        Node::fixed("A", [0.0, 0.0, 0.0]),
        Node::fixed("B", [6.0, 0.0, 0.0]),
        Node::free("C", [0.0, 0.0, 5.0]),
        Node::free("D", [6.0, 0.0, 5.0]),
        Node::free("M", [3.0, 0.0, 5.0]),
    ];
    m.members = vec![
        Member::new("c1", MemberKind::Frame, 0, 2, 0),
        Member::new("c2", MemberKind::Frame, 1, 3, 0),
        Member::new("b1", MemberKind::Frame, 2, 4, 1),
        Member::new("b2", MemberKind::Frame, 4, 3, 1),
    ];
    point(&mut m, "P1", 2, [0.0; 3]);
    point(&mut m, "P2", 4, [0.0; 3]);
    let attachments = vec![
        Attachment::of_type("P1"),
        // Relinked by the placed structure to D, 0.8 m ahead of it.
        Attachment {
            label: "P2".into(),
            link: Link::Joint {
                joint: "D".into(),
                offset: [0.0, 0.8, 0.0],
            },
        },
    ];
    let c = condense(
        &m,
        &attachments,
        &Linearisation::Unloaded,
        &default_budget(),
    )
    .unwrap();
    assert_eq!(c.places[1].node, 3);
    assert_eq!(c.places[1].offset, [0.0, 0.8, 0.0]);
    let first = Options {
        nonlinear: false,
        ..Options::default()
    };
    for (q, a) in attachments.iter().enumerate() {
        for j in 0..3 {
            let mut e = [0.0; 3];
            e[j] = 1.0;
            let load = m.attachment_load(&a.label, &a.link, e).unwrap();
            let s = solve(
                &m,
                &Loads {
                    nodal: load.loads,
                    self_weight: 0.0,
                    ..Loads::default()
                },
                &first,
                &default_budget(),
            )
            .unwrap();
            for (p, place) in c.places.iter().enumerate() {
                let u = place_displacement(s.displacements[place.node], place.offset);
                for (i, ui) in u.iter().enumerate() {
                    let cij = c.flexibility[(3 * p + i, 3 * q + j)];
                    assert!(
                        (cij - ui).abs() <= 1e-9 * c.flexibility.abs().max(),
                        "C[{}][{}] {cij:e} against the full solve {ui:e}",
                        3 * p + i,
                        3 * q + j,
                    );
                }
            }
        }
    }
    let k = c.stiffness.as_ref().expect("positive definite");
    let unit = k * &c.flexibility;
    for r in 0..6 {
        for s in 0..6 {
            let want = if r == s { 1.0 } else { 0.0 };
            assert!((unit[(r, s)] - want).abs() < 1e-8, "{unit}");
        }
    }
    assert!(c.flexibility.clone().cholesky().is_some());
}

/// At a reference state with P-Delta, the condensed flexibility is the
/// tangent: a column under a vertical load P at its top is softer across
/// than the elastic one, and matches the finite difference of two
/// non-linear solves around the reference.
#[test]
fn the_tangent_at_a_reference_matches_finite_differences() {
    let (l, a, i) = (12.0, 4e-3, 3e-5);
    let mut m = column(&[4.0, 8.0, l], a, i);
    point(&mut m, "top", 3, [0.0; 3]);
    let p = -0.25 * std::f64::consts::PI.powi(2) * E * i / (4.0 * l * l);
    let reference = |h: f64| Loads {
        nodal: vec![NodalLoad {
            node: 3,
            force: [h, 0.0, p],
            moment: [0.0; 3],
        }],
        self_weight: 0.0,
        ..Loads::default()
    };
    let c = condense(
        &m,
        &points(&["top"]),
        &Linearisation::Tangent(reference(0.0)),
        &default_budget(),
    )
    .unwrap();
    let elastic = l.powi(3) / (3.0 * E * i);
    // A quarter of the buckling load: about 1/(1 − 0.25) softer.
    let ratio = c.flexibility[(0, 0)] / elastic;
    assert!(ratio > 1.25 && ratio < 1.45, "{ratio}");
    let h = 10.0;
    let solve_at = |f: f64| {
        solve(&m, &reference(f), &Options::default(), &default_budget())
            .unwrap()
            .displacements[3][0]
    };
    let fd = (solve_at(h) - solve_at(-h)) / (2.0 * h);
    assert!(
        close(c.flexibility[(0, 0)], fd, 1e-6),
        "tangent {:e} against finite differences {fd:e}",
        c.flexibility[(0, 0)]
    );
}

/// One X panel of tension-only diagonals: at a reference state where a
/// lateral load leaves one diagonal slack, the condensation takes the
/// panel with that diagonal out — the flexibility of the model without
/// it — and the unloaded one both diagonals in.
#[test]
fn the_tension_only_set_is_the_reference_state() {
    let panel = |both: bool| {
        let mut m = Model {
            sections: vec![Section::elastic("s", E, 1e-3, 0.0)],
            ..Model::default()
        };
        m.nodes = vec![
            Node::pinned("A", [0.0, 0.0, 0.0]),
            Node::pinned("B", [4.0, 0.0, 0.0]),
            Node::free("C", [0.0, 0.0, 3.0]),
            Node::free("D", [4.0, 0.0, 3.0]),
        ];
        m.members = vec![
            Member::new("AC", MemberKind::Truss, 0, 2, 0),
            Member::new("BD", MemberKind::Truss, 1, 3, 0),
            Member::new("CD", MemberKind::Truss, 2, 3, 0),
            Member::new("AD", MemberKind::Tension, 0, 3, 0),
        ];
        if both {
            m.members
                .push(Member::new("BC", MemberKind::Tension, 1, 2, 0));
        }
        // Held out of the plane at C and D (a plane truss).
        for n in [2, 3] {
            m.nodes[n].fixed[1] = true;
        }
        point(&mut m, "P", 3, [0.0; 3]);
        m
    };
    let m = panel(true);
    // Pushed towards +x at D: AD stretched, BC shortened (slack: no
    // second moment, no buckling load).
    let pushed = Loads {
        nodal: vec![NodalLoad {
            node: 3,
            force: [5e3, 0.0, 0.0],
            moment: [0.0; 3],
        }],
        self_weight: 0.0,
        ..Loads::default()
    };
    let at_ref = condense(
        &m,
        &points(&["P"]),
        &Linearisation::Tangent(pushed),
        &default_budget(),
    )
    .unwrap();
    let without = panel(false);
    let alone = condense(
        &without,
        &points(&["P"]),
        &Linearisation::Unloaded,
        &default_budget(),
    )
    .unwrap();
    let both = condense(
        &m,
        &points(&["P"]),
        &Linearisation::Unloaded,
        &default_budget(),
    )
    .unwrap();
    // P-Delta of 5 kN in a 3–4 m panel changes the stiffness by far
    // less than 1 %; the set is what matters.
    let (r, a, b) = (
        at_ref.flexibility[(0, 0)],
        alone.flexibility[(0, 0)],
        both.flexibility[(0, 0)],
    );
    assert!(
        close(r, a, 1e-3),
        "at the reference {r:e}, without BC {a:e}"
    );
    assert!(b < 0.8 * a, "both diagonals {b:e} against one {a:e}");
}

/// A placed structure turned by its orientation: the turned matrices
/// give, for a force in line axes, the displacement of the structure's
/// own matrices turned back (R C Rᵀ), and keep their eigenvalues.
#[test]
fn a_turned_structure_deflects_as_turned() {
    let mut m = column(&[10.0], 4e-3, 3e-5);
    m.sections[0].iz = 1e-5; // weaker about z: different stiffness each way
    point(&mut m, "A", 1, [1.0, 0.5, 0.0]);
    let c = condense(
        &m,
        &points(&["A"]),
        &Linearisation::Unloaded,
        &default_budget(),
    )
    .unwrap();
    let a = 30f64.to_radians();
    let (s, co) = a.sin_cos();
    // Structure axes in line axes: x → (cos, sin), y → (−sin, cos).
    let r = [[co, -s, 0.0], [s, co, 0.0], [0.0, 0.0, 1.0]];
    let turned = c.turned(r);
    let f_line = [3e3, -1e3, 2e3];
    let u_line = turned.deflection(&[("A", f_line)])[0];
    // By hand: into the structure's axes (Rᵀ), deflected, back (R).
    let f_local = [
        co * f_line[0] + s * f_line[1],
        -s * f_line[0] + co * f_line[1],
        f_line[2],
    ];
    let u_local = c.deflection(&[("A", f_local)])[0];
    let back = [
        co * u_local[0] - s * u_local[1],
        s * u_local[0] + co * u_local[1],
        u_local[2],
    ];
    for k in 0..3 {
        assert!((u_line[k] - back[k]).abs() < 1e-12, "{u_line:?} {back:?}");
    }
    let ev = |x: &nalgebra::DMatrix<f64>| {
        let mut v: Vec<f64> = x
            .clone()
            .symmetric_eigen()
            .eigenvalues
            .iter()
            .copied()
            .collect();
        v.sort_by(f64::total_cmp);
        v
    };
    for (p, q) in ev(&c.flexibility).iter().zip(ev(&turned.flexibility)) {
        assert!(close(q, *p, 1e-9));
    }
}

fn uniform_pole(elasticity: Option<f64>) -> Pole {
    Pole {
        id: "p".into(),
        name: "uniform test pole".into(),
        material: PoleMaterial::Wood,
        source: None,
        length: 14.0,
        embedment: 2.0,
        top_diameter: 0.3,
        ground_diameter: 0.3,
        wall: None,
        fibre_stress: Some(50e6),
        yield_strength: None,
        bend_radius: None,
        code: PoleCode::default(),
        strength_class: None,
        moisture_class: None,
        moment_capacity: None,
        capacity: Vec::new(),
        elasticity,
        density: 600.0,
        drag: 1.0,
        sides: None,
        profile: Vec::new(),
    }
}

/// A pole of uniform section: its condensed flexibility at a point at
/// height a is the cantilever's a³/(3EI) across, and a crossarm offset e
/// adds the twist e²·a/(GJ) (J = 2I, G = E/2.6); without an elasticity
/// it stays rigid, said.
#[test]
fn a_pole_condenses_to_the_cantilever() {
    let e_wood = 11e9;
    let pole = uniform_pole(Some(e_wood));
    let i = std::f64::consts::PI / 64.0 * 0.3f64.powi(4);
    let ei = e_wood * i;
    let gj = e_wood / 2.6 * 2.0 * i;
    let pts = vec![
        ("top".to_owned(), [0.0, 0.0, 12.0]),
        ("arm".to_owned(), [1.2, 0.0, 10.5]),
    ];
    let c = condense_pole(&pole, &pts, None, &default_budget()).unwrap();
    let top = c.flexibility_between("top", "top").unwrap();
    assert!(
        close(top[(0, 0)], 12f64.powi(3) / (3.0 * ei), 1e-9),
        "{top}"
    );
    assert!(close(top[(1, 1)], 12f64.powi(3) / (3.0 * ei), 1e-9));
    let arm = c.flexibility_between("arm", "arm").unwrap();
    let a: f64 = 10.5;
    assert!(
        close(
            arm[(1, 1)],
            a.powi(3) / (3.0 * ei) + 1.2 * 1.2 * a / gj,
            1e-9
        ),
        "{arm}"
    );
    assert!(c.notes.iter().any(|n| n.contains("NOT ON THE SAFE SIDE")));
    // With its weight and the wires at a reference: softer (P-Delta).
    let wires = [PoleLoad {
        point: [0.0, 0.0, 12.0],
        force: [0.0, 0.0, -20e3],
    }];
    let loaded = condense_pole(&pole, &pts, Some(&wires), &default_budget()).unwrap();
    let soft = loaded.flexibility_between("top", "top").unwrap();
    assert!(soft[(0, 0)] > top[(0, 0)] * 1.01, "{soft} {top}");
    assert_eq!(
        condense_pole(&uniform_pole(None), &pts, None, &default_budget()),
        Err(Rigid::PoleWithoutElasticity)
    );
}

/// Structures without a usable model stay rigid, with the reason.
#[test]
fn structures_without_a_usable_model_stay_rigid() {
    let mut m = column(&[10.0], 4e-3, 3e-5);
    point(&mut m, "A", 1, [0.0; 3]);
    let budget = default_budget();
    let unlinked = vec![Attachment {
        label: "A".into(),
        link: Link::Unlinked,
    }];
    let r = condense(&m, &unlinked, &Linearisation::Unloaded, &budget).unwrap_err();
    assert_eq!(r, Rigid::Unlinked("A".into()));
    assert_eq!(r.to_string(), "attachment point A not linked to a joint");
    let r = condense(&m, &points(&["B"]), &Linearisation::Unloaded, &budget).unwrap_err();
    assert_eq!(r, Rigid::NotInModel("B".into()));
    assert_eq!(
        condense(&m, &[], &Linearisation::Unloaded, &budget),
        Err(Rigid::NoPoints)
    );
    assert_eq!(
        Rigid::NoModel.to_string(),
        "no structure model (detail level 1 or 2)"
    );
    // A point on a joint inserted in a straight bar: free across it.
    let mut bar = Model {
        sections: vec![Section::elastic("s", E, 1e-3, 0.0)],
        ..Model::default()
    };
    bar.nodes = vec![
        Node::pinned("a", [0.0, 0.0, 0.0]),
        Node::free("m", [1.0, 0.0, 0.0]),
        Node::pinned("b", [2.0, 0.0, 0.0]),
    ];
    bar.members = vec![
        Member::new("1", MemberKind::Truss, 0, 1, 0),
        Member::new("2", MemberKind::Truss, 1, 2, 0),
    ];
    point(&mut bar, "M", 1, [0.0; 3]);
    let r = condense(&bar, &points(&["M"]), &Linearisation::Unloaded, &budget).unwrap_err();
    assert!(
        matches!(&r, Rigid::FreeAcross { label, joint } if label == "M" && joint == "m"),
        "{r}"
    );
    // A reference that does not solve: rigid, with the solver's error.
    let mut weak = m.clone();
    weak.analysis.nonlinear = true;
    let crushing = Loads {
        nodal: vec![NodalLoad {
            node: 1,
            force: [0.0, 0.0, -1e9],
            moment: [0.0; 3],
        }],
        self_weight: 0.0,
        ..Loads::default()
    };
    let r = condense(
        &weak,
        &points(&["A"]),
        &Linearisation::Tangent(crushing),
        &budget,
    )
    .unwrap_err();
    assert!(matches!(r, Rigid::Reference(_)), "{r}");
    assert!(
        r.to_string().contains("did not solve at the reference"),
        "{r}"
    );
    // An exit clause stops the unit solves: said, never a hang.
    let token = ohl_core::budget::CancelToken::new();
    token.cancel();
    let cancelled = Budget::new().cancel_token(token);
    let r = condense(&m, &points(&["A"]), &Linearisation::Unloaded, &cancelled).unwrap_err();
    assert!(matches!(r, Rigid::Reference(_) | Rigid::Stopped(_)), "{r}");
}

/// A force at a point that a joint held to the ground would carry (a
/// bar hanging from the column's top, tensioned at the reference, its
/// free end held across it by the stabilisation: its geometric
/// stiffness pulls that end sideways) refuses the structure as flexible.
#[test]
fn a_point_leaning_on_a_ground_hold_is_refused() {
    // The column's top: 3EI/L³ = 18.9 kN/m across; the bar's pull of
    // 2 kN over 1 m: 2 kN/m of geometric stiffness, about 10 % of a
    // force at the top held at J.
    let mut m = column(&[10.0], 4e-3, 3e-5);
    m.sections.push(Section::elastic("bar", E, 1e-4, 0.0));
    m.nodes.push(Node::free("J", [1.0, 0.0, 10.0]));
    m.members
        .push(Member::new("bar", MemberKind::Truss, 1, 2, 1));
    point(&mut m, "A", 1, [0.0; 3]);
    let pulled = Loads {
        nodal: vec![NodalLoad {
            node: 2,
            force: [2e3, 0.0, 0.0],
            moment: [0.0; 3],
        }],
        self_weight: 0.0,
        ..Loads::default()
    };
    let r = condense(
        &m,
        &points(&["A"]),
        &Linearisation::Tangent(pulled),
        &default_budget(),
    )
    .unwrap_err();
    assert!(
        matches!(&r, Rigid::HeldToGround { label, joint, .. } if label == "A" && joint == "J"),
        "{r}"
    );
}

/// The memo's key: the same inputs give the same key; an edited model,
/// another extension or another reference another.
#[test]
fn the_key_follows_every_input() {
    let mut m = column(&[10.0], 4e-3, 3e-5);
    point(&mut m, "A", 1, [0.0; 3]);
    let pts = points(&["A"]);
    let lin = Linearisation::Unloaded;
    let k = CondenseKey::of_model("T", None, &m, &pts, &lin);
    assert_eq!(k, CondenseKey::of_model("T", None, &m.clone(), &pts, &lin));
    assert_ne!(k, CondenseKey::of_model("T", Some("+3"), &m, &pts, &lin));
    let mut edited = m.clone();
    edited.sections[0].area *= 1.000001;
    assert_ne!(k, CondenseKey::of_model("T", None, &edited, &pts, &lin));
    let loaded = Linearisation::Tangent(Loads {
        self_weight: 1.0,
        ..Loads::default()
    });
    assert_ne!(k, CondenseKey::of_model("T", None, &m, &pts, &loaded));
}

/// Several points on one node share a place (one block); the example
/// pylon (type-02, a full design with stabilised joints) condenses at
/// its self-weight to a symmetric, positive definite matrix.
#[test]
fn points_on_one_node_share_a_place_and_the_example_pylon_condenses() {
    let mut m = column(&[10.0], 4e-3, 3e-5);
    point(&mut m, "A", 1, [0.0; 3]);
    point(&mut m, "B", 1, [0.0; 3]);
    let c = condense(
        &m,
        &points(&["A", "B"]),
        &Linearisation::Unloaded,
        &default_budget(),
    )
    .unwrap();
    assert_eq!(c.places.len(), 1);
    assert_eq!(c.place_of, vec![0, 0]);
    assert!(c.stiffness.is_some());
    let u = c.deflection(&[("A", [1e3, 0.0, 0.0]), ("B", [1e3, 0.0, 0.0])]);
    assert_eq!(u[0], u[1]);

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/synthetic-line/Structures/type-02.ohls");
    let pylon = Model::load(&path).unwrap();
    let labels: Vec<&str> = pylon
        .joint_points
        .iter()
        .map(|p| p.label.as_str())
        .collect();
    assert!(labels.len() >= 2, "{labels:?}");
    let started = std::time::Instant::now();
    let c = condense(
        &pylon,
        &points(&labels),
        &Linearisation::Tangent(Loads {
            self_weight: 1.0,
            ..Loads::default()
        }),
        &default_budget(),
    )
    .unwrap();
    eprintln!(
        "example pylon: {} members, {} points condensed in {:?}",
        pylon.members.len(),
        labels.len(),
        started.elapsed()
    );
    let f = &c.flexibility;
    assert!((f - f.transpose()).abs().max() < 1e-12 * f.abs().max());
    assert!(c.stiffness.is_some(), "{:?}", c.notes);
    // Every direction gives: a positive diagonal.
    for d in 0..f.nrows() {
        assert!(f[(d, d)] > 0.0);
    }
}

/// A generated lattice tower of about 1000 members with 24 points at
/// its top levels condenses quickly at a reference state (self-weight
/// and loads at the points, P-Delta): the time printed (the limit here
/// is generous for unoptimised test builds).
#[test]
fn a_thousand_member_tower_condenses_quickly() {
    let mut m = Model {
        sections: vec![Section::elastic("s", E, 2e-3, 1e-6)],
        ..Model::default()
    };
    m.sections[0].density = 7850.0;
    let levels = 56;
    for k in 0..=levels {
        let z = k as f64 * 1.5;
        let w = 4.0 - 2.5 * k as f64 / levels as f64;
        for (cx, cy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let id = format!("n{k}-{cx}{cy}");
            let p = [cx * w, cy * w, z];
            m.nodes.push(if k == 0 {
                Node::pinned(&id, p)
            } else {
                Node::free(&id, p)
            });
        }
    }
    let at = |k: usize, c: usize| 4 * k + c % 4;
    let mut add = |a: usize, b: usize| {
        let id = format!("m{}", m.members.len());
        m.members.push(Member::new(&id, MemberKind::Truss, a, b, 0));
    };
    for k in 0..levels {
        for c in 0..4 {
            add(at(k, c), at(k + 1, c));
            add(at(k, c), at(k + 1, c + 1));
            add(at(k, c + 1), at(k + 1, c));
            add(at(k + 1, c), at(k + 1, c + 1));
            if c < 2 {
                add(at(k + 1, c), at(k + 1, c + 2));
            }
        }
    }
    assert!(m.members.len() > 900);
    let mut labels = Vec::new();
    for k in (levels - 5)..=levels {
        for c in 0..4 {
            let label = format!("P{k}-{c}");
            point(&mut m, &label, at(k, c), [0.0; 3]);
            labels.push(label);
        }
    }
    let wires = Loads {
        nodal: (0..4)
            .map(|c| NodalLoad {
                node: at(levels, c),
                force: [1e3, 5e3, -10e3],
                moment: [0.0; 3],
            })
            .collect(),
        self_weight: 1.0,
        ..Loads::default()
    };
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let started = std::time::Instant::now();
    let c = condense(
        &m,
        &points(&refs),
        &Linearisation::Tangent(wires),
        &default_budget(),
    )
    .unwrap();
    let took = started.elapsed();
    eprintln!(
        "{} members, {} points ({} unit solves): condensed in {took:?}",
        m.members.len(),
        labels.len(),
        3 * c.places.len()
    );
    assert!(took.as_secs_f64() < 10.0, "{took:?}");
    assert_eq!(c.places.len(), 24);
    assert!(c.stiffness.is_some());
}
