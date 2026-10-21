// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Textbook checks of the solver and the pole check.

use std::f64::consts::PI;

use ohl_core::budget::Budget;

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

fn nodal(loads: Vec<NodalLoad>) -> Loads {
    Loads {
        nodal: loads,
        self_weight: 0.0,
    }
}

/// A cantilever of `n` frame elements from the origin along `dir`.
fn cantilever(n: usize, l: f64, dir: [f64; 3], section: Section) -> Model {
    let mut m = Model {
        sections: vec![section],
        ..Model::default()
    };
    for k in 0..=n {
        let t = l * k as f64 / n as f64;
        let p = [dir[0] * t, dir[1] * t, dir[2] * t];
        m.nodes.push(if k == 0 {
            Node::fixed("base", p)
        } else {
            Node::free(&format!("n{k}"), p)
        });
    }
    for k in 0..n {
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

/// Tip deflection PL³/3EI and rotation PL²/2EI, in both bending planes,
/// for a member along x and along a skew direction (one element is exact).
#[test]
fn cantilever_tip_deflection() {
    let (l, p) = (4.0, 1000.0);
    let mut s = Section::elastic("s", E, 1e-3, 2e-6);
    s.iy = 3e-6;
    let m = cantilever(1, l, [1.0, 0.0, 0.0], s.clone());
    let sol = solve(
        &m,
        &nodal(vec![load(1, [0.0, 0.0, -p])]),
        &first_order(),
        &default_budget(),
    )
    .unwrap();
    let d = sol.displacements[1];
    // Load along −z bends about the local y axis (local y is global z for
    // a horizontal member: the x–y plane is vertical, Iz).
    assert!(
        close(d[2], -p * l.powi(3) / (3.0 * E * s.iz), 1e-12),
        "{d:?}"
    );
    assert!(close(d[4].abs(), p * l * l / (2.0 * E * s.iz), 1e-12));
    let sol = solve(
        &m,
        &nodal(vec![load(1, [0.0, p, 0.0])]),
        &first_order(),
        &default_budget(),
    )
    .unwrap();
    assert!(close(
        sol.displacements[1][1],
        p * l.powi(3) / (3.0 * E * s.iy),
        1e-12
    ));
    // Reactions: the load back, and the moment P·L.
    let r = sol.reactions[0];
    assert!(
        close(r.force[1], -p, 1e-12) && close(r.moment[2], -p * l, 1e-12),
        "{r:?}"
    );
    // Skew member, symmetric section: the deflection along the load.
    let c = 1.0 / 3f64.sqrt();
    let m = cantilever(3, l, [c, c, c], Section::elastic("s", E, 1e-3, 2e-6));
    let sol = solve(
        &m,
        &nodal(vec![load(3, [-p * c, -p * c, 2.0 * p * c])]),
        &first_order(),
        &default_budget(),
    )
    .unwrap();
    let d = sol.displacements[3];
    let f = 2f64.sqrt() * p; // |F| (perpendicular to the member)
    let unit = [-1.0 / 6f64.sqrt(), -1.0 / 6f64.sqrt(), 2.0 / 6f64.sqrt()];
    let along = d[0] * unit[0] + d[1] * unit[1] + d[2] * unit[2];
    assert!(
        close(along, f * l.powi(3) / (3.0 * E * 2e-6), 1e-9),
        "{d:?}"
    );
}

/// A bar under axial load: u = PL/EA; tension positive.
#[test]
fn axial_bar() {
    let (l, p, a) = (5.0, 2e5, 2e-3);
    let mut m = Model {
        sections: vec![Section::elastic("s", E, a, 0.0)],
        ..Model::default()
    };
    m.nodes = vec![
        Node::pinned("a", [0.0, 0.0, 0.0]),
        Node {
            fixed: [false, true, true, false, false, false],
            ..Node::free("b", [l, 0.0, 0.0])
        },
    ];
    m.members = vec![Member::new("bar", MemberKind::Truss, 0, 1, 0)];
    let sol = solve(
        &m,
        &nodal(vec![load(1, [p, 0.0, 0.0])]),
        &first_order(),
        &default_budget(),
    )
    .unwrap();
    assert!(close(sol.displacements[1][0], p * l / (E * a), 1e-12));
    assert!(close(sol.members[0].axial, p, 1e-12));
    assert!(close(sol.reactions[0].force[0], -p, 1e-12));
}

/// The triangle truss: two rafters at 45° under a ridge load P carry P/√2
/// in compression, the tie P/2 in tension; each support P/2 up.
#[test]
fn triangle_truss_member_forces() {
    let p = 10e3;
    let mut m = Model {
        sections: vec![Section::elastic("s", E, 1e-3, 0.0)],
        ..Model::default()
    };
    m.nodes = vec![
        Node::pinned("A", [0.0, 0.0, 0.0]),
        Node {
            fixed: [false, true, true, false, false, false],
            ..Node::free("B", [4.0, 0.0, 0.0])
        },
        Node {
            fixed: [false, true, false, false, false, false],
            ..Node::free("C", [2.0, 0.0, 2.0])
        },
    ];
    m.members = vec![
        Member::new("AB", MemberKind::Truss, 0, 1, 0),
        Member::new("AC", MemberKind::Truss, 0, 2, 0),
        Member::new("BC", MemberKind::Truss, 1, 2, 0),
    ];
    let sol = solve(
        &m,
        &nodal(vec![load(2, [0.0, 0.0, -p])]),
        &first_order(),
        &default_budget(),
    )
    .unwrap();
    assert!(close(sol.members[0].axial, p / 2.0, 1e-12));
    assert!(close(sol.members[1].axial, -p / 2f64.sqrt(), 1e-12));
    assert!(close(sol.members[2].axial, -p / 2f64.sqrt(), 1e-12));
    let a = sol.reactions.iter().find(|r| r.node == 0).unwrap();
    assert!(
        close(a.force[2], p / 2.0, 1e-12) && a.force[0].abs() < 1e-6,
        "{a:?}"
    );
    // Usage: the rafters buckle first (Euler, k = 1) when given an I.
    m.sections[0].i_min = 1e-7;
    m.sections[0].yield_strength = Some(355e6);
    let u = member_usage(&m, &sol, 1.0);
    let euler = PI * PI * E * 1e-7 / 8.0;
    assert!(
        close(u[1].usage.unwrap(), p / 2f64.sqrt() / euler, 1e-12),
        "{u:?}"
    );
    assert!(u[1].governs.contains("buckling"));
    assert!(close(u[0].usage.unwrap(), p / 2.0 / (355e6 * 1e-3), 1e-12));
    assert!(close(u[2].usage.unwrap(), u[1].usage.unwrap(), 1e-9));
    assert!(check::worst(&u).unwrap().member >= 1);
}

/// P-Delta: a cantilever column under P = Pcr/2 and a tip load H; the
/// exact beam-column gives δ = H(tan kL − kL)/(Pk), M = H·tan(kL)/k
/// (Timoshenko & Gere, "Theory of Elastic Stability", §1.11) — about
/// 1/(1 − P/Pcr) = 2 times the first-order values.
#[test]
fn p_delta_amplification_of_a_cantilever() {
    let (l, h) = (10.0, 1000.0);
    let i = 5e-6;
    let pcr = PI * PI * E * i / (4.0 * l * l);
    let p = 0.5 * pcr;
    let m = cantilever(8, l, [0.0, 0.0, 1.0], Section::elastic("s", E, 1e-2, i));
    let loads = nodal(vec![load(8, [h, 0.0, -p])]);
    let lin = solve(&m, &loads, &first_order(), &default_budget()).unwrap();
    let nl = solve(&m, &loads, &Options::default(), &default_budget()).unwrap();
    let k = (p / (E * i)).sqrt();
    let exact = h * ((k * l).tan() - k * l) / (p * k);
    let d0 = lin.displacements[8][0];
    assert!(close(d0, h * l.powi(3) / (3.0 * E * i), 1e-9));
    let d = nl.displacements[8][0];
    assert!(close(d, exact, 2e-4), "{d} vs {exact}");
    assert!(close(d / d0, 1.0 / (1.0 - p / pcr), 0.01), "{}", d / d0);
    let base = nl.reactions[0].moment[1].abs();
    assert!(close(base, h * (k * l).tan() / k, 2e-4), "{base}");
    // The column's axial force is known after the first solve: the second
    // one is final.
    assert!(nl.iterations >= 2 && nl.residual < 1e-9);
    // Beyond the buckling load: unstable.
    let over = nodal(vec![load(8, [h, 0.0, -1.05 * pcr])]);
    assert!(matches!(
        solve(&m, &over, &Options::default(), &default_budget()),
        Err(FeError::Unstable { .. })
    ));
    // Exit clause: the error says why it stopped and how far it got.
    let e = solve(
        &m,
        &loads,
        &Options::default(),
        &Budget::new().max_iterations(1),
    )
    .unwrap_err();
    let FeError::Stopped { residual, .. } = &e else {
        panic!("{e}")
    };
    assert!(*residual > 1e-9);
    assert!(e.to_string().contains("1 iterations"), "{e}");
}

/// A stiff mast held at the top by two pretensioned guys: both work until
/// the leeward one goes slack at H = 2·T0·cos α, then the windward one
/// alone (cos α = horizontal share of a guy's direction).
#[test]
fn guy_pretension() {
    let (a, ht, t0): (f64, f64, f64) = (6.0, 8.0, 20e3);
    let ea = 2e8;
    let lg = (a * a + ht * ht).sqrt();
    let cos = a / lg;
    let mut m = Model {
        sections: vec![
            // A nearly rigid mast: the guys keep their pretension.
            Section::elastic("mast", E, 1e4, 0.0),
            Section::elastic("guy", ea, 1.0, 0.0),
        ],
        ..Model::default()
    };
    m.nodes = vec![
        Node::pinned("base", [0.0, 0.0, 0.0]),
        Node {
            fixed: [false, true, false, false, false, false],
            ..Node::free("top", [0.0, 0.0, ht])
        },
        Node::pinned("left", [-a, 0.0, 0.0]),
        Node::pinned("right", [a, 0.0, 0.0]),
    ];
    let mut gl = Member::new("guy-left", MemberKind::Cable, 2, 1, 1);
    gl.pretension = t0;
    let mut gr = Member::new("guy-right", MemberKind::Cable, 3, 1, 1);
    gr.pretension = t0;
    m.members = vec![Member::new("mast", MemberKind::Truss, 0, 1, 0), gl, gr];
    let k = ea / lg * cos * cos;
    // Small load: both guys taut.
    let h = 10e3;
    let sol = solve(
        &m,
        &nodal(vec![load(1, [h, 0.0, 0.0])]),
        &first_order(),
        &default_budget(),
    )
    .unwrap();
    let u = sol.displacements[1][0];
    assert!(close(u, h / (2.0 * k), 1e-6), "{u}");
    assert!(close(sol.members[1].axial, t0 + h / (2.0 * cos), 1e-6));
    assert!(close(sol.members[2].axial, t0 - h / (2.0 * cos), 1e-6));
    // The pretension compresses the mast.
    assert!(sol.members[0].axial < -2.0 * t0 * (ht / lg) * 0.99);
    // Beyond 2·T0·cos α the leeward guy is slack.
    let h = 3.0 * t0 * cos;
    let sol = solve(
        &m,
        &nodal(vec![load(1, [h, 0.0, 0.0])]),
        &first_order(),
        &default_budget(),
    )
    .unwrap();
    assert!(sol.members[2].slack && sol.members[2].axial == 0.0);
    let u = sol.displacements[1][0];
    assert!(close(u, (h / cos - t0) / (ea / lg * cos), 1e-6), "{u}");
    assert!(close(sol.members[1].axial, h / cos, 1e-6));
    // Without pretension the leeward guy is slack at once: twice as soft.
    for g in &mut m.members[1..] {
        g.pretension = 0.0;
    }
    let h = 10e3;
    let sol = solve(
        &m,
        &nodal(vec![load(1, [h, 0.0, 0.0])]),
        &first_order(),
        &default_budget(),
    )
    .unwrap();
    assert!(close(sol.displacements[1][0], h / k, 1e-6));
}

/// Self-weight: a horizontal cantilever's tip deflection wL⁴/8EI and the
/// reactions carry the whole weight.
#[test]
fn self_weight() {
    let l = 6.0;
    let mut s = Section::elastic("s", E, 4e-3, 8e-6);
    s.density = 7850.0;
    let m = cantilever(6, l, [1.0, 0.0, 0.0], s.clone());
    let sol = solve(
        &m,
        &Loads {
            nodal: vec![],
            self_weight: 1.0,
        },
        &first_order(),
        &default_budget(),
    )
    .unwrap();
    let w = s.weight();
    assert!(close(
        sol.displacements[6][2],
        -w * l.powi(4) / (8.0 * E * s.iz),
        1e-9
    ));
    assert!(close(sol.reactions[0].force[2], w * l, 1e-9));
    assert!(close(m.weight(), w * l, 1e-12));
    // Moment at the base wL²/2.
    assert!(close(
        sol.reactions[0].moment[1].abs(),
        w * l * l / 2.0,
        1e-9
    ));
}

/// A node held by nothing in one direction is reported by name (without
/// stabilisation); stabilised, a bar pulled along its axis solves with its
/// free end held across it, and pulled across it is refused.
#[test]
fn mechanisms_are_named() {
    let mut m = Model {
        sections: vec![Section::elastic("s", E, 1e-3, 0.0)],
        ..Model::default()
    };
    m.nodes = vec![
        Node::pinned("a", [0.0, 0.0, 0.0]),
        Node::free("b", [1.0, 0.0, 0.0]),
    ];
    m.members = vec![Member::new("bar", MemberKind::Truss, 0, 1, 0)];
    let along = nodal(vec![load(1, [1.0, 0.0, 0.0])]);
    let e = solve(
        &m,
        &along,
        &Options {
            stabilise: false,
            ..first_order()
        },
        &default_budget(),
    )
    .unwrap_err();
    assert!(
        matches!(&e, FeError::Mechanism { node, .. } if node == "b"),
        "{e}"
    );
    assert_eq!(
        e.to_string(),
        "mechanism: node b is free to move in y (add a member, support or spring)"
    );
    let s = solve(&m, &along, &first_order(), &default_budget()).unwrap();
    assert!(close(s.members[0].axial, 1.0, 1e-12));
    let held: Vec<String> = s.stabilised.iter().map(|h| h.at.dof_name()).collect();
    assert_eq!(held, ["y", "z"]);
    let across = nodal(vec![load(1, [0.0, 0.0, -1.0])]);
    let e = solve(&m, &across, &first_order(), &default_budget()).unwrap_err();
    assert!(
        matches!(&e, FeError::Held { node, dof, .. } if node == "b" && dof == "z"),
        "{e}"
    );
}

/// A generated lattice tower of about 1000 members solves quickly (the
/// limit here is generous for unoptimised test builds).
#[test]
fn a_thousand_members_solve_quickly() {
    let mut m = Model {
        sections: vec![Section::elastic("s", E, 2e-3, 1e-6)],
        ..Model::default()
    };
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
            add(at(k, c), at(k + 1, c)); // leg
            add(at(k, c), at(k + 1, c + 1)); // diagonals
            add(at(k, c + 1), at(k + 1, c));
            add(at(k + 1, c), at(k + 1, c + 1)); // horizontal
            if c < 2 {
                add(at(k + 1, c), at(k + 1, c + 2)); // plan bracing
            }
        }
    }
    assert!(m.members.len() > 900, "{}", m.members.len());
    let top: Vec<NodalLoad> = (0..4)
        .map(|c| load(at(levels, c), [5e3, 2e3, -10e3]))
        .collect();
    let started = std::time::Instant::now();
    let sol = solve(&m, &nodal(top), &Options::default(), &default_budget()).unwrap();
    let took = started.elapsed();
    assert!(took.as_secs_f64() < 10.0, "{took:?}");
    // Equilibrium: the reactions balance the loads.
    let sum: f64 = sol.reactions.iter().map(|r| r.force[0]).sum();
    assert!(close(sum, -20e3, 1e-6), "{sum}");
}

/// The shipped example pylon's members (in its structure type file)
/// parse; its points on joints map to nodes.
#[test]
fn example_model_parses() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/synthetic-line/Structures/type-02.ohls");
    let m = Model::load(&path).unwrap();
    assert!(m.attachment_node("A1").is_some() && m.attachment_node("A11").is_some());
    let (sol, usage) = analyse(
        &m,
        &Loads {
            nodal: m.loads.clone(),
            self_weight: 1.0,
        },
        1.0,
        &default_budget(),
    )
    .unwrap();
    assert!(sol.residual < 1e-9);
    let w = check::worst(&usage).unwrap();
    assert!(w.usage.unwrap() > 0.0 && w.usage.unwrap() < 1.0, "{w:?}");
    // Bad files are refused with a reason.
    for bad in [
        "[[node]]\nid = \"a\"\npoint = [0, 0, 0]\nsupport = \"glued\"\n",
        "format = \"x\"\n",
        "[[material]]\nid = \"s\"\nelasticity = \"1 GPa\"\n[[section]]\nid = \"a\"\nmaterial = \"t\"\narea = \"1 cm2\"\n",
    ] {
        assert!(Model::parse(bad).is_err(), "{bad}");
    }
}

/// A point a file places along a member (`member`, `at`, written before
/// 2026-10-21) is on a joint inserted there, the member split into two
/// continuous pieces of its properties and buckling length (owner
/// decision 2026-10-21). An L frame (column fixed at the base, beam out
/// along x) loaded at a quarter of the beam gives the reactions by hand
/// (P up, −P·1 m about y) and the solution of the same frame with the
/// joint drawn there, bit for bit. Along a truss member the two bars
/// meet at the joint with no stiffness across them: the solve is refused
/// with the fix; a frame, or a member to the joint in the plane of the
/// load, carries it.
#[test]
fn point_along_a_member_is_on_a_joint_inserted_there() {
    let head = "[analysis]\nnonlinear = false\nself_weight = false\n\
                [[material]]\nid = \"steel\"\nelasticity = \"210 GPa\"\nyield = \"355 MPa\"\n\
                [[section]]\nid = \"s\"\nmaterial = \"steel\"\narea = \"2000 mm2\"\n\
                iy = \"500 cm4\"\niz = \"500 cm4\"\nwy = \"50 cm3\"\nwz = \"50 cm3\"\n\
                [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"fixed\"\n\
                [[node]]\nid = \"B\"\npoint = [0, 0, 4]\n\
                [[node]]\nid = \"C\"\npoint = [4, 0, 4]\n\
                [[member]]\nid = \"col\"\nnodes = [\"A\", \"B\"]\nsection = \"s\"\nkind = \"frame\"\n";
    let along = format!(
        "{head}[[member]]\nid = \"beam\"\nnodes = [\"B\", \"C\"]\nsection = \"s\"\nkind = \"frame\"\n\
         [[point]]\nlabel = \"P1\"\nmember = \"beam\"\nat = 0.25\n"
    );
    // The joint drawn, each piece buckling over the whole beam (k × 4 m).
    let joint = format!(
        "{head}[[node]]\nid = \"P1\"\npoint = [1, 0, 4]\n\
         [[member]]\nid = \"beam\"\nnodes = [\"B\", \"P1\"]\nsection = \"s\"\nkind = \"frame\"\nlk = \"4 m\"\n\
         [[member]]\nid = \"beam-2\"\nnodes = [\"P1\", \"C\"]\nsection = \"s\"\nkind = \"frame\"\nlk = \"4 m\"\n\
         [[point]]\nlabel = \"P1\"\nnode = \"P1\"\n"
    );
    let p = 10e3;
    let solved = |text: &str| {
        let m = Model::from_structure_type(text).unwrap();
        let at = m.point_loads("P1", [0.0, 0.0, -p]).unwrap();
        let sol = solve(&m, &nodal(at), &first_order(), &default_budget());
        (m, sol)
    };
    let (m, read) = solved(&along);
    let read = read.unwrap();
    let r = read.reactions[0];
    assert!(close(r.force[2], p, 1e-9), "{r:?}");
    assert!(close(r.moment[1], -p * 1.0, 1e-9), "{r:?}");
    let (mj, drawn) = solved(&joint);
    let drawn = drawn.unwrap();
    // The same model solved: every number the same.
    assert_eq!(m.nodes.len(), 4);
    assert_eq!(m.node_name(3), "P1");
    assert_eq!(m.members.len(), 3);
    assert_eq!(m.members[2].id, "beam-2");
    assert_eq!(m.members[1].lk, Some(4.0));
    assert_eq!(read.reactions, drawn.reactions);
    assert_eq!(read.displacements, drawn.displacements);
    assert_eq!(read.members, drawn.members);
    assert_eq!(member_usage(&m, &read, 1.0), member_usage(&mj, &drawn, 1.0));
    assert!(m.point_loads("P2", [0.0; 3]).is_none());
    // Along a truss member: two bars in a line at the joint, the load
    // across them refused with the fix.
    let truss = along.replace(
        "nodes = [\"B\", \"C\"]\nsection = \"s\"\nkind = \"frame\"",
        "nodes = [\"B\", \"C\"]\nsection = \"s\"\nkind = \"truss\"",
    );
    let (m, refused) = solved(&truss);
    assert_eq!(m.bar_joint_free(3).len(), 2);
    let e = refused.unwrap_err().to_string();
    assert!(
        e.contains("joint P1 (attachment point P1): only bars meet it")
            && e.contains("make the members frames, or add a member to the joint"),
        "{e}"
    );
    // Along the bars it is carried.
    let at = m.point_loads("P1", [p, 0.0, 0.0]).unwrap();
    assert!(solve(&m, &nodal(at), &first_order(), &default_budget()).is_ok());
    // A member to the joint in the plane of the load (x–z): carried.
    let braced = format!(
        "{truss}[[member]]\nid = \"brace\"\nnodes = [\"A\", \"P1\"]\nsection = \"s\"\nkind = \"truss\"\n"
    );
    let (m, ok) = solved(&braced);
    assert_eq!(m.bar_joint_free(3).len(), 1);
    assert!(ok.is_ok(), "{ok:?}");
    // A point on a member that is not there is refused.
    let bad = along.replace("member = \"beam\"", "member = \"nope\"");
    assert!(Model::from_structure_type(&bad).is_err());
}

/// Two attachment points on one joint (2026-10-21): their loads add up;
/// a point at an offset adds the moment r × F where a frame meets the
/// joint, and only a note where trusses alone do.
#[test]
fn points_share_joints_and_offsets_make_moments() {
    let base = "[analysis]\nnonlinear = false\nself_weight = false\n\
                [[material]]\nid = \"steel\"\nelasticity = \"210 GPa\"\n\
                [[section]]\nid = \"s\"\nmaterial = \"steel\"\narea = \"2000 mm2\"\n\
                iy = \"500 cm4\"\niz = \"500 cm4\"\n\
                [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"fixed\"\n\
                [[node]]\nid = \"T\"\npoint = [3, 0, 0]\n\
                [[member]]\nid = \"arm\"\nnodes = [\"A\", \"T\"]\nsection = \"s\"\nkind = \"frame\"\n\
                [[point]]\nlabel = \"P\"\nnode = \"T\"\n\
                [[point]]\nlabel = \"Q\"\nnode = \"T\"\noffset = [\"0 m\", \"0.5 m\", \"0 m\"]\n";
    let m = Model::from_structure_type(base).unwrap();
    assert_eq!(m.attachment_node("P"), Some(1));
    assert_eq!(m.attachment_node("Q"), Some(1));
    let f = [0.0, 0.0, -2000.0];
    let p = m.point_load("P", f).unwrap();
    let q = m.point_load("Q", f).unwrap();
    assert_eq!(p.moment, [0.0; 3]);
    // r × F = (0, 0.5, 0) × (0, 0, −2000) = (−1000, 0, 0) N·m.
    assert_eq!(q.moment, [-1000.0, 0.0, 0.0]);
    assert!(q.moment_applied && q.note.is_none());
    let mut loads = p.loads.clone();
    loads.extend(q.loads.clone());
    let sol = solve(&m, &nodal(loads), &first_order(), &default_budget()).unwrap();
    let r = sol.reactions[0];
    assert!(close(r.force[2], 4000.0, 1e-12), "{r:?}");
    assert!(close(r.moment[0], 1000.0, 1e-9), "{r:?}");
    // The same moment by the linked lookup a placed structure uses.
    let l = m.linked_load("Q", "T", [0.0, 0.5, 0.0], f).unwrap();
    assert_eq!(l, q);
    // Trusses only at the joint: the moment left out, said so.
    let truss = base.replace("kind = \"frame\"", "kind = \"truss\"");
    let m = Model::from_structure_type(&truss).unwrap();
    let q = m.point_load("Q", f).unwrap();
    assert!(!q.moment_applied);
    assert_eq!(q.loads[0].moment, [0.0; 3]);
    assert!(q.note.unwrap().contains("make it a frame joint"));
}

/// Wood pole: a horizontal load F at height h gives F·h at the ground
/// line against f·πD³/32; the wind on a linear taper adds
/// p·(D₀H²/2 + (D₁ − D₀)H²/3); a vertical load amplifies the moment.
#[test]
fn pole_check() {
    let text = "id = \"p\"\nmaterial = \"wood\"\nlength = \"12 m\"\nembedment = \"2 m\"\n\
                top_diameter = \"200 mm\"\nground_diameter = \"300 mm\"\nfibre_stress = \"50 MPa\"\n";
    let pole = Pole::parse(text).unwrap();
    let f = 5e3;
    let loads = [PoleLoad {
        point: [0.0, 0.0, 9.0],
        force: [f, 0.0, 0.0],
    }];
    let c = check_pole(&pole, &loads, [0.0, 0.0], 1.0, false, &default_budget()).unwrap();
    assert!(close(c.ground_moment, f * 9.0, 1e-12));
    // The taper: the critical section may be above ground; at least the
    // ground-line ratio.
    let cap0 = 50e6 * PI * 0.3f64.powi(3) / 32.0;
    assert!(c.usage >= f * 9.0 / cap0 - 1e-12);
    // Wind only.
    let p = 700.0;
    let c = check_pole(&pole, &[], [p, 0.0], 1.0, false, &default_budget()).unwrap();
    let hh: f64 = 10.0;
    let exact = p * (0.3 * hh * hh / 2.0 + (0.2 - 0.3) * hh * hh / 3.0);
    assert!(
        close(c.wind_moment, exact, 1e-4),
        "{} {exact}",
        c.wind_moment
    );
    // Offset vertical load: moment V·x.
    let c = check_pole(
        &pole,
        &[PoleLoad {
            point: [1.5, 0.0, 9.0],
            force: [0.0, 0.0, -4e3],
        }],
        [0.0, 0.0],
        1.0,
        false,
        &default_budget(),
    )
    .unwrap();
    assert!(close(c.ground_moment, 6e3, 1e-12));
    // P-Delta with the elasticity: amplified, roughly 1/(1 − P/Pcr).
    let stiff = Pole::parse(&format!("{text}elasticity = \"10 GPa\"\n")).unwrap();
    let mut loads = loads.to_vec();
    loads.push(PoleLoad {
        point: [0.0, 0.0, 10.0],
        force: [0.0, 0.0, -10e3],
    });
    let c = check_pole(&stiff, &loads, [0.0, 0.0], 1.0, true, &default_budget()).unwrap();
    assert!(c.amplification > 1.01 && c.amplification < 1.5, "{c:?}");
    // Strength factor and a capacity table.
    let steel = Pole::parse(
        "id = \"s\"\nmaterial = \"steel\"\nlength = \"15 m\"\nembedment = \"2 m\"\n\
         top_diameter = \"200 mm\"\nground_diameter = \"400 mm\"\nmoment_capacity = \"100 kN m\"\n",
    )
    .unwrap();
    let c = check_pole(
        &steel,
        &[PoleLoad {
            point: [0.0, 0.0, 10.0],
            force: [0.0, 4e3, 0.0],
        }],
        [0.0, 0.0],
        0.8,
        false,
        &default_budget(),
    )
    .unwrap();
    assert!(close(c.usage, 40e3 / (0.8 * 100e3), 1e-12), "{c:?}");
    assert!(Pole::parse("id = \"x\"\nmaterial = \"steel\"\nlength = \"10 m\"\ntop_diameter = \"1 m\"\nground_diameter = \"1 m\"\n").is_err());
}

/// A section library of invented profiles: a tube and a rectangular tube
/// (300 mm along z, 100 mm along y).
const LIBRARY: &str = "[[material]]\nid = \"steel\"\nelasticity = \"210 GPa\"\n\
    shear_modulus = \"81 GPa\"\ndensity = \"7850 kg/m3\"\nyield = \"355 MPa\"\n\
    [[section]]\nid = \"T200\"\nshape = \"circular_hollow\"\nmaterial = \"steel\"\n\
    d = \"200 mm\"\nt = \"8 mm\"\n\
    [[section]]\nid = \"R300\"\nshape = \"rectangular_hollow\"\nmaterial = \"steel\"\n\
    h = \"300 mm\"\nb = \"100 mm\"\nt = \"8 mm\"\n";

/// A frame portal of `section` (two columns and a beam, fixed feet),
/// loaded at a top corner; `extra`: more tables.
fn portal(section: &str, extra: &str) -> String {
    format!(
        "[analysis]\nnonlinear = true\n{extra}\
         [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"fixed\"\n\
         [[node]]\nid = \"B\"\npoint = [0, 0, 6]\n\
         [[node]]\nid = \"C\"\npoint = [5, 0, 6]\n\
         [[node]]\nid = \"D\"\npoint = [5, 0, 0]\nsupport = \"fixed\"\n\
         [[member]]\nid = \"c1\"\nnodes = [\"A\", \"B\"]\nsection = \"{section}\"\nkind = \"frame\"\n\
         [[member]]\nid = \"beam\"\nnodes = [\"B\", \"C\"]\nsection = \"{section}\"\nkind = \"frame\"\n\
         [[member]]\nid = \"c2\"\nnodes = [\"D\", \"C\"]\nsection = \"{section}\"\nkind = \"frame\"\n\
         [[load]]\nnode = \"B\"\nforce = [\"20 kN\", \"5 kN\", \"-30 kN\"]\n"
    )
}

/// A member whose section is in the section library gives the same
/// results as with a `[[section]]` table of the same values; the library
/// comes first; an unknown id is refused, named.
#[test]
fn library_sections_resolve_first() {
    let lib = ohl_sections::SectionLibrary::parse(LIBRARY).unwrap();
    let q = lib.section("T200").unwrap().properties();
    let table = format!(
        "[[material]]\nid = \"steel\"\nelasticity = 210e9\nshear_modulus = 81e9\n\
         density = 7850.0\nyield = 355e6\n\
         [[section]]\nid = \"local\"\nmaterial = \"steel\"\narea = {:e}\niy = {:e}\niz = {:e}\n\
         torsion = {:e}\ni_min = {:e}\nwy = {:e}\nwz = {:e}\n",
        q.area,
        q.iy,
        q.iz,
        q.it.unwrap(),
        q.iv,
        q.wel_y.unwrap(),
        q.wel_z.unwrap(),
    );
    let run = |m: &Model| {
        let loads = Loads {
            nodal: m.loads.clone(),
            self_weight: 1.0,
        };
        analyse(m, &loads, 1.0, &default_budget()).unwrap()
    };
    let from_lib = Model::parse_with(&portal("T200", ""), &lib).unwrap();
    let local = Model::parse(&portal("local", &table)).unwrap();
    assert!(from_lib.sections[0].profile.is_some() && local.sections[0].profile.is_none());
    assert!(close(from_lib.weight(), local.weight(), 1e-12));
    let ((a, ua), (b, ub)) = (run(&from_lib), run(&local));
    for (da, db) in a.displacements.iter().zip(&b.displacements) {
        for k in 0..6 {
            assert!(
                (da[k] - db[k]).abs() <= 1e-9 * db[k].abs().max(1e-6),
                "{da:?} {db:?}"
            );
        }
    }
    for (x, y) in ua.iter().zip(&ub) {
        assert!(
            close(x.usage.unwrap(), y.usage.unwrap(), 1e-9),
            "{x:?} {y:?}"
        );
        assert_eq!(x.governs, y.governs);
    }
    // A table of the same id: the library's is used.
    let shadowed = table.replace("id = \"local\"", "id = \"T200\"");
    let m = Model::parse_with(&portal("T200", &shadowed), &lib).unwrap();
    assert_eq!(m.sections.len(), 1);
    assert!(m.sections[0].profile.is_some());
    // Without the library: the table.
    let m = Model::parse(&portal("T200", &shadowed)).unwrap();
    assert!(m.sections[0].profile.is_none());
    // Unknown everywhere: refused, named.
    let e = Model::parse_with(&portal("NOPE", ""), &lib).unwrap_err();
    assert!(e.contains("NOPE") && e.contains("c1"), "{e}");
    // The library's material missing: named.
    let lonely = ohl_sections::SectionLibrary::parse(
        "[[section]]\nid = \"X\"\nshape = \"round_bar\"\nmaterial = \"gone\"\nd = \"20 mm\"\n",
    )
    .unwrap();
    let e = Model::parse_with(&portal("X", ""), &lonely).unwrap_err();
    assert!(e.contains("gone"), "{e}");
}

/// Member wind on a vertical cantilever: pressure × drag × width × length
/// in all, half at each joint; the base reacts with the force and q·d·L²/2
/// (the uniform load's moment). The width follows the wind's direction in
/// the section (300 mm across y, 100 mm across z), and a wind at 45° to the
/// member gives sin²θ of it.
#[test]
fn member_wind_on_a_cantilever() {
    let lib = ohl_sections::SectionLibrary::parse(LIBRARY).unwrap();
    let text = |section: &str, drag: &str| {
        format!(
            "[analysis]\nnonlinear = false\nself_weight = false\n{drag}\
             [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"fixed\"\n\
             [[node]]\nid = \"B\"\npoint = [0, 0, 10]\n\
             [[member]]\nid = \"m\"\nnodes = [\"A\", \"B\"]\nsection = \"{section}\"\nkind = \"frame\"\n"
        )
    };
    let (q, l) = (500.0, 10.0);
    let base = |m: &Model, wind: Vec<NodalLoad>| {
        let sol = solve(m, &nodal(wind), &first_order(), &default_budget()).unwrap();
        sol.reactions[0]
    };
    // A tube: 200 mm whatever the direction.
    let m = Model::parse_with(&text("T200", ""), &lib).unwrap();
    assert!(m.has_profiles());
    let wind = m.member_wind(q, [1.0, 0.0, 0.0]);
    assert_eq!(wind.len(), 2);
    for w in &wind {
        assert!(close(w.force[0], 0.5 * q * 0.2 * l, 1e-12), "{w:?}");
    }
    let r = base(&m, wind);
    assert!(close(r.force[0], -q * 0.2 * l, 1e-9), "{r:?}");
    assert!(close(r.moment[1], -q * 0.2 * l * l / 2.0, 1e-9), "{r:?}");
    // Drag factor 1.5.
    let m = Model::parse_with(&text("T200", "drag = 1.5\n"), &lib).unwrap();
    let r = base(&m, m.member_wind(q, [1.0, 0.0, 0.0]));
    assert!(close(r.force[0], -1.5 * q * 0.2 * l, 1e-9), "{r:?}");
    // The rectangular tube: a vertical member's local y is the structure's
    // x, so a wind along x sees the 300 mm side, along y the 100 mm one.
    let m = Model::parse_with(&text("R300", ""), &lib).unwrap();
    let r = base(&m, m.member_wind(q, [1.0, 0.0, 0.0]));
    assert!(close(r.force[0], -q * 0.3 * l, 1e-9), "{r:?}");
    let r = base(&m, m.member_wind(q, [0.0, -1.0, 0.0]));
    assert!(close(r.force[1], q * 0.1 * l, 1e-9), "{r:?}");
    // A wind at 45° to the member: half of it (sin²θ), normal to the member.
    let m = Model::parse_with(&text("T200", ""), &lib).unwrap();
    let r = base(&m, m.member_wind(q, [1.0, 0.0, 1.0]));
    assert!(close(r.force[0], -0.5 * q * 0.2 * l, 1e-9), "{r:?}");
    assert!(r.force[2].abs() < 1e-9, "{r:?}");
    // Along the member: none. A table section: none.
    assert!(m.member_wind(q, [0.0, 0.0, 1.0]).is_empty());
    let table = "[[material]]\nid = \"s\"\nelasticity = \"210 GPa\"\n\
                 [[section]]\nid = \"t\"\nmaterial = \"s\"\narea = \"10 cm2\"\niy = \"100 cm4\"\n\
                 iz = \"100 cm4\"\n";
    let m = Model::parse(&format!("{table}{}", text("t", ""))).unwrap();
    assert!(!m.has_profiles() && m.member_wind(q, [1.0, 0.0, 0.0]).is_empty());
    assert!(Model::parse_with(&text("T200", "drag = 0\n"), &lib).is_err());
}

/// Member wind on a horizontal truss bar between two pins, the wind
/// across it: each support takes half of q·d·L.
#[test]
fn member_wind_on_a_truss_bar() {
    let lib = ohl_sections::SectionLibrary::parse(LIBRARY).unwrap();
    let text = "[analysis]\nnonlinear = false\nself_weight = false\n\
        [[node]]\nid = \"A\"\npoint = [0, 0, 5]\nsupport = \"pinned\"\n\
        [[node]]\nid = \"B\"\npoint = [0, 8, 5]\nsupport = \"pinned\"\n\
        [[member]]\nid = \"bar\"\nnodes = [\"A\", \"B\"]\nsection = \"T200\"\n";
    let m = Model::from_structure_type_with(text, &lib).unwrap();
    let wind = m.member_wind(700.0, [-1.0, 0.0, 0.0]);
    let sol = solve(&m, &nodal(wind), &first_order(), &default_budget()).unwrap();
    for r in &sol.reactions {
        assert!(close(r.force[0], 0.5 * 700.0 * 0.2 * 8.0, 1e-9), "{r:?}");
    }
}
