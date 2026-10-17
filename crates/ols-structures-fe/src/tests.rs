// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Textbook checks of the solver and the pole check.

use std::f64::consts::PI;

use ols_core::budget::Budget;

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

/// A node held by nothing in one direction is reported by name.
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
    let e = solve(
        &m,
        &nodal(vec![load(1, [1.0, 0.0, 0.0])]),
        &first_order(),
        &default_budget(),
    )
    .unwrap_err();
    assert!(
        matches!(&e, FeError::Mechanism { node, .. } if node == "b"),
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
        .join("../../examples/synthetic-line/Structures/type-02.olss");
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

/// A point along a member (`member`, `at`) shares its load to the
/// member's ends: an L frame (column fixed at the base, beam out along x)
/// loaded at a quarter of the beam gives the reactions by hand (P up, −P·1 m
/// about y) and those of the same frame with a joint at that point.
#[test]
fn point_along_a_member_carries_its_load() {
    let head = "[analysis]\nnonlinear = false\nself_weight = false\n\
                [[material]]\nid = \"steel\"\nelasticity = \"210 GPa\"\n\
                [[section]]\nid = \"s\"\nmaterial = \"steel\"\narea = \"2000 mm2\"\n\
                iy = \"500 cm4\"\niz = \"500 cm4\"\n\
                [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"fixed\"\n\
                [[node]]\nid = \"B\"\npoint = [0, 0, 4]\n\
                [[node]]\nid = \"C\"\npoint = [4, 0, 4]\n\
                [[member]]\nid = \"col\"\nnodes = [\"A\", \"B\"]\nsection = \"s\"\nkind = \"frame\"\n";
    let along = format!(
        "{head}[[member]]\nid = \"beam\"\nnodes = [\"B\", \"C\"]\nsection = \"s\"\nkind = \"frame\"\n\
         [[point]]\nlabel = \"P1\"\nmember = \"beam\"\nat = 0.25\n"
    );
    let joint = format!(
        "{head}[[node]]\nid = \"D\"\npoint = [1, 0, 4]\n\
         [[member]]\nid = \"b1\"\nnodes = [\"B\", \"D\"]\nsection = \"s\"\nkind = \"frame\"\n\
         [[member]]\nid = \"b2\"\nnodes = [\"D\", \"C\"]\nsection = \"s\"\nkind = \"frame\"\n\
         [[point]]\nlabel = \"P1\"\nnode = \"D\"\n"
    );
    let p = 10e3;
    let reaction = |text: &str| {
        let m = Model::from_structure_type(text).unwrap();
        let at = m.point_loads("P1", [0.0, 0.0, -p]).unwrap();
        let sol = solve(&m, &nodal(at), &first_order(), &default_budget()).unwrap();
        sol.reactions[0]
    };
    let shared = reaction(&along);
    assert!(close(shared.force[2], p, 1e-9), "{shared:?}");
    assert!(close(shared.moment[1], -p * 1.0, 1e-9), "{shared:?}");
    let exact = reaction(&joint);
    for k in 0..3 {
        assert!((shared.force[k] - exact.force[k]).abs() < 1e-6 * p);
        assert!((shared.moment[k] - exact.moment[k]).abs() < 1e-6 * p);
    }
    // The shares: (1 − at) to the first joint, at to the second.
    let m = Model::from_structure_type(&along).unwrap();
    let shares = m.point_loads("P1", [0.0, 0.0, -p]).unwrap();
    assert_eq!(shares.len(), 2);
    assert!(close(shares[0].force[2], -0.75 * p, 1e-12));
    assert!(close(shares[1].force[2], -0.25 * p, 1e-12));
    assert!(m.point_loads("P2", [0.0; 3]).is_none());
    // A point on a member that is not there is refused.
    let bad = along.replace("member = \"beam\"", "member = \"nope\"");
    assert!(Model::from_structure_type(&bad).is_err());
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
