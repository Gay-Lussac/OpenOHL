// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tension-only members on invented models: an X-braced panel whose
//! compressed diagonal goes slack (against the hand solution of the
//! panel with one diagonal), the load reversed, a crossing joint held
//! only by slack halves, P-Delta, the cycle and pass limits of the
//! active-set iterations, models whose tension-only members stay taut
//! solving exactly as trusses; compression up to the members' own
//! buckling load (the X panel's compressed diagonal bowed, carrying it,
//! against the method of joints; a stocky one a truss); and the formerly
//! cycling kinked joint settling (its ties split), with the runs that
//! split ties and, as the last resort, keep members active.

use crate::solve::{TENSION_PASSES, next_active_set, rounds, settle, solve_stabilised};
use crate::{
    FeError, Loads, Member, MemberKind, Model, NodalLoad, Node, Options, Section, Solution,
    default_budget, kept_active_note, member_usage, solve, tension_only_note,
};

const E: f64 = 2.1e11;
/// The horizontal load on the panel, N.
const H: f64 = 10e3;

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

/// A panel 4 m wide and 3 m high in the x–z plane: feet A (0, 0) and B
/// (4, 0) pinned, top joints D (0, 3) and C (4, 3); legs AD, BC, a top
/// DC (trusses) and the diagonals AC, BD of `kind` — each in two halves
/// through a crossing joint M at the middle when `crossing`. Nothing
/// holds the panel across its plane: the joint stabilisation does.
fn panel(kind: MemberKind, crossing: bool) -> Model {
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
        Member::new("DC", MemberKind::Truss, 3, 2, 0),
    ];
    if crossing {
        m.nodes.push(Node::free("M", [2.0, 0.0, 1.5]));
        for (id, i, j) in [("AM", 0, 4), ("MC", 4, 2), ("BM", 1, 4), ("MD", 4, 3)] {
            m.members.push(Member::new(id, kind, i, j, 0));
        }
    } else {
        m.members.push(Member::new("AC", kind, 0, 2, 0));
        m.members.push(Member::new("BD", kind, 1, 3, 0));
    }
    m
}

fn index(m: &Model, id: &str) -> usize {
    m.members.iter().position(|x| x.id == id).unwrap()
}

fn axial(m: &Model, s: &Solution, id: &str) -> f64 {
    s.members[index(m, id)].axial
}

fn solved(m: &Model, nodal: Vec<NodalLoad>, options: &Options) -> Solution {
    let loads = Loads {
        nodal,
        self_weight: 0.0,
    };
    solve(m, &loads, options, &default_budget()).unwrap_or_else(|e| panic!("{e}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(H)
}

/// H at D along +x: AC stretched, BD shortened. The hand solution of the
/// panel with AC alone (statically determinate; method of joints): AC
/// 5H/4, the top DC −H, the leg BC −3H/4, the leg AD 0.
#[test]
fn x_panel_one_diagonal_carries_the_shear() {
    let m = panel(MemberKind::Tension, false);
    let s = solved(&m, vec![force(3, [H, 0.0, 0.0])], &first_order());
    assert!(close(axial(&m, &s, "AC"), 1.25 * H), "{s:?}");
    let bd = &s.members[index(&m, "BD")];
    assert!(bd.slack && bd.axial == 0.0 && bd.end_forces == [0.0; 12]);
    assert!(!s.members[index(&m, "AC")].slack);
    assert!(close(axial(&m, &s, "DC"), -H));
    assert!(close(axial(&m, &s, "BC"), -0.75 * H));
    assert!(axial(&m, &s, "AD").abs() < 1e-9 * H);
    // Solved with both diagonals, then with BD left out: settled.
    assert_eq!(s.passes, 2);
    // Equilibrium: the feet take −H.
    let sum: f64 = s.reactions.iter().map(|r| r.force[0]).sum();
    assert!(close(sum, -H), "{sum}");
    // Its usage: 0, slack.
    let mut steel = m.clone();
    steel.sections[0].yield_strength = Some(355e6);
    let u = member_usage(&steel, &s, 1.0);
    let bd = &u[index(&m, "BD")];
    assert_eq!((bd.usage, bd.governs.as_str()), (Some(0.0), "slack"));
    assert!(u[index(&m, "AC")].usage.unwrap() > 0.0);
    assert_eq!(
        tension_only_note(&m, &s).unwrap(),
        "tension-only members: 1 of 2 slack (2 solve(s)): BD"
    );
    // The same panel with truss diagonals: both carry, no passes, no note.
    let t = panel(MemberKind::Truss, false);
    let st = solved(&t, vec![force(3, [H, 0.0, 0.0])], &first_order());
    assert_eq!(st.passes, 0);
    assert!(tension_only_note(&t, &st).is_none());
    assert!(axial(&t, &st, "AC") > 0.1 * H && axial(&t, &st, "BD") < -0.1 * H);
}

/// The load reversed: BD carries it (5H/4), AC is slack; the leg AD
/// −3H/4, the top and BC nothing.
#[test]
fn reversing_the_load_flips_the_diagonals() {
    let m = panel(MemberKind::Tension, false);
    let s = solved(&m, vec![force(3, [-H, 0.0, 0.0])], &first_order());
    assert!(close(axial(&m, &s, "BD"), 1.25 * H), "{s:?}");
    assert!(s.members[index(&m, "AC")].slack);
    assert!(axial(&m, &s, "DC").abs() < 1e-9 * H);
    assert!(close(axial(&m, &s, "AD"), -0.75 * H));
    assert!(axial(&m, &s, "BC").abs() < 1e-9 * H);
}

/// Diagonals in halves through a crossing joint M: the compressed halves
/// go slack, M is then held only by the taut diagonal's halves (on one
/// line), so the solve holds it across that line — in the plane (a skew
/// direction) and across the plane (y) — with no force, M carrying no
/// load: the forces of the panel without the joint, both slack halves
/// left out (each shortened by half of the diagonal's shortening).
#[test]
fn a_crossing_joint_between_slack_halves_is_held_without_force() {
    let m = panel(MemberKind::Tension, true);
    // Each diagonal's halves are one tie (AM, MC; BM, MD).
    assert_eq!(crate::solve::chains(&m), [vec![3, 4], vec![5, 6]]);
    let s = solved(&m, vec![force(3, [H, 0.0, 0.0])], &first_order());
    for id in ["AM", "MC"] {
        assert!(close(axial(&m, &s, id), 1.25 * H), "{id}: {s:?}");
    }
    for id in ["BM", "MD"] {
        assert!(s.members[index(&m, id)].slack, "{id}");
    }
    assert!(close(axial(&m, &s, "DC"), -H));
    assert!(close(axial(&m, &s, "BC"), -0.75 * H));
    let at_m: Vec<_> = s.stabilised.iter().filter(|h| h.at.node == 4).collect();
    assert!(at_m.iter().any(|h| h.at.axis == Some(1)), "{at_m:?}");
    assert!(at_m.iter().any(|h| h.at.axis.is_none()), "{at_m:?}");
    let largest = s
        .stabilised
        .iter()
        .map(|h| h.force.abs())
        .fold(0.0, f64::max);
    assert!(largest < 1e-9 * H, "{largest}");
    // The same with the load reversed: the other halves.
    let r = solved(&m, vec![force(3, [-H, 0.0, 0.0])], &first_order());
    for id in ["BM", "MD"] {
        assert!(close(axial(&m, &r, id), 1.25 * H), "{id}: {r:?}");
    }
    assert!(r.members[index(&m, "AM")].slack && r.members[index(&m, "MC")].slack);
}

/// A heavy vertical load on the panel's top shortens its legs and with
/// them both diagonals: the first solve compresses both. Left out
/// together, nothing would hold the panel's sway (a mechanism); the more
/// compressed one alone left out, the other is stretched by the sway and
/// carries the shear, as in the hand solution (the vertical load in the
/// legs).
#[test]
fn diagonals_both_shortened_leave_one_at_a_time() {
    let m = panel(MemberKind::Tension, false);
    let loads = vec![force(3, [H, 0.0, -200e3]), force(2, [0.0, 0.0, -200e3])];
    // Both diagonals truss: both compressed.
    let t = panel(MemberKind::Truss, false);
    let st = solved(&t, loads.clone(), &first_order());
    assert!(axial(&t, &st, "AC") < 0.0 && axial(&t, &st, "BD") < axial(&t, &st, "AC"));
    let s = solved(&m, loads, &first_order());
    assert!(close(axial(&m, &s, "AC"), 1.25 * H), "{s:?}");
    assert!(s.members[index(&m, "BD")].slack);
    assert!(close(axial(&m, &s, "BC"), -200e3 - 0.75 * H));
    assert!(close(axial(&m, &s, "AD"), -200e3));
    // All active; both out (a mechanism); BD out: settled.
    assert_eq!(s.passes, 3);
}

/// With P-Delta: the set found first order (BD slack), then the panel
/// solved with the geometric stiffness of its active members — the
/// numbers of the panel without BD, as a truss model, with P-Delta.
#[test]
fn p_delta_solves_the_active_members() {
    let m = panel(MemberKind::Tension, false);
    let loads = vec![force(3, [H, 0.0, -200e3]), force(2, [0.0, 0.0, -200e3])];
    let s = solved(&m, loads.clone(), &Options::default());
    assert!(s.nonlinear && s.members[index(&m, "BD")].slack);
    // Three solves first order (above), one with P-Delta.
    assert_eq!(s.passes, 4);
    let mut without = panel(MemberKind::Truss, false);
    without.members.remove(index(&without, "BD"));
    let w = solved(&without, loads.clone(), &Options::default());
    for id in ["AD", "BC", "DC", "AC"] {
        let (a, b) = (axial(&m, &s, id), axial(&without, &w, id));
        assert!((a - b).abs() <= 1e-9 * b.abs().max(H), "{id}: {a} {b}");
    }
    // The legs' compression sways the panel more than first order.
    let f = solved(&m, loads, &first_order());
    assert!(s.displacements[3][0] > 1.001 * f.displacements[3][0]);
}

/// Tension-only members that stay taut: the model solves with exactly
/// the numbers of the same model with trusses — first order and P-Delta
/// (the same equations solved the same way).
#[test]
fn taut_tension_members_solve_exactly_as_trusses() {
    // The X panel with its top pulled up: both diagonals stretched.
    let loads = vec![force(3, [0.0, 0.0, H]), force(2, [0.0, 0.0, H])];
    for options in [first_order(), Options::default()] {
        let t = solved(&panel(MemberKind::Tension, false), loads.clone(), &options);
        let r = solved(&panel(MemberKind::Truss, false), loads.clone(), &options);
        assert!(
            t.members[3].axial > 0.0 && t.members[4].axial > 0.0,
            "{t:?}"
        );
        assert_eq!(t.displacements, r.displacements);
        assert_eq!(t.members, r.members);
        assert_eq!(t.reactions, r.reactions);
        assert_eq!(t.passes, if options.nonlinear { 2 } else { 1 });
        assert_eq!(r.passes, 0);
    }
}

/// The kind in files: `tension` in a model file (version 2) and in a
/// structure file's members; a pretension refused on it; a model file of
/// a newer version refused.
#[test]
fn tension_members_in_files() {
    let body = "[[material]]\nid = \"s\"\nelasticity = \"210 GPa\"\n\
                [[section]]\nid = \"a\"\nmaterial = \"s\"\narea = \"10 cm2\"\n\
                [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"pinned\"\n\
                [[node]]\nid = \"B\"\npoint = [0, 0, 5]\n\
                [[member]]\nid = \"t\"\nnodes = [\"A\", \"B\"]\nsection = \"a\"\nkind = \"tension\"\n";
    let m = Model::parse(&format!(
        "format = \"ohl-structure-model\"\nversion = 2\n{body}"
    ))
    .unwrap();
    assert_eq!(m.members[0].kind, MemberKind::Tension);
    assert_eq!(m.members[0].kind.as_str(), "tension");
    let s = Model::from_structure_type(&format!(
        "id = \"T\"\nname = \"t\"\nkind = \"lattice\"\n{body}"
    ))
    .unwrap();
    assert_eq!(s.members[0].kind, MemberKind::Tension);
    let e = Model::parse(&format!("{body}pretension = \"1 kN\"\n")).unwrap_err();
    assert!(e.contains("only cables take a pretension"), "{e}");
    let newer = format!(
        "format = \"ohl-structure-model\"\nversion = {}\n{body}",
        crate::model::MODEL_FILE_VERSION + 1
    );
    assert!(Model::parse(&newer).unwrap_err().contains("newer"));
}

/// The active-set iterations stop on a set solved before (a cycle),
/// naming the members that go taut and slack in turn, and after
/// [`TENSION_PASSES`] solves when the set keeps changing.
#[test]
fn unsettled_sets_are_refused_naming_the_members() {
    let m = panel(MemberKind::Tension, false);
    // A step that flips AC and BD in turn.
    let mut k = 0;
    let e = settle(&m, false, |_, inactive| {
        k += 1;
        let mut next = inactive.to_vec();
        next[3] = k % 2 == 1;
        next[4] = k % 2 == 0;
        Ok(((), next, vec![0.0; 5]))
    })
    .unwrap_err();
    let FeError::Unsettled {
        members,
        flipping,
        passes,
        cycle,
        indices,
    } = &e
    else {
        panic!("{e}");
    };
    assert!(*cycle && *passes == 3, "{e}");
    assert_eq!(
        (members.as_slice(), *flipping, indices.as_slice()),
        (&["AC".to_owned(), "BD".to_owned()][..], 2, &[3, 4][..])
    );
    assert!(
        e.to_string().contains("AC, BD go taut and slack in turn"),
        "{e}"
    );
    // A step whose sets never repeat (a binary count over the seven
    // members of the panel with a crossing joint).
    let x = panel(MemberKind::Tension, true);
    let mut k = 0u32;
    let e = settle(&x, true, |_, _| {
        k += 1;
        Ok(((), (0..7).map(|b| k >> b & 1 == 1).collect(), vec![0.0; 7]))
    })
    .unwrap_err();
    assert!(
        matches!(&e, FeError::Unsettled { cycle: false, passes, .. } if *passes == TENSION_PASSES),
        "{e}"
    );
    // A settled set: its solution and the solves, both phases.
    let mut k = 0;
    let (last, passes) = settle(&m, true, |nonlinear, inactive| {
        k += 1;
        let next = (0..5).map(|b| b == 4 || inactive[b]).collect();
        Ok(((k, nonlinear), next, vec![0.0; 5]))
    })
    .unwrap();
    assert_eq!((last, passes), ((3, true), 3));
    // Members left out together failing the solve: tried again with the
    // more compressed half, down to one, whose failure is the error.
    let mut tried: Vec<Vec<bool>> = Vec::new();
    let e = settle(&x, false, |_, inactive| {
        tried.push(inactive.to_vec());
        if inactive.iter().any(|&b| b) {
            return Err(mechanism());
        }
        // The four halves compressed, MD most, then BM, MC, AM.
        Ok((
            (),
            vec![false, false, false, true, true, true, true],
            vec![0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0],
        ))
    })
    .unwrap_err();
    assert!(matches!(e, FeError::Mechanism { .. }), "{e}");
    let out = |set: &Vec<bool>| (0..7).filter(|&b| set[b]).collect::<Vec<_>>();
    let sets: Vec<Vec<usize>> = tried.iter().map(out).collect();
    assert_eq!(sets, [vec![], vec![3, 4, 5, 6], vec![5, 6], vec![6]]);
}

fn mechanism() -> FeError {
    FeError::Mechanism {
        node: "M".into(),
        dof: "x",
        joints: vec!["M".into()],
        moving: 1,
    }
}

/// A pair of tension-only diagonals AC and BD of a 4 m × 3 m frame,
/// each in two halves through a joint M 1 mm off the line of AC (so AM,
/// MC are still one tie, as are BM, MD), the corners pinned, M held
/// otherwise only by a weak spring (1 kN/m), the section's second moment
/// `i`: with the halves taken out, M is nearly free. Pulled along the
/// frame, the iterations with the halves switched as ties go round (the
/// halves of AC taut and slack in turn) — the pattern seen on a tower
/// whose diagonals left a part nearly free.
fn kinked_joint(kind: MemberKind, i: f64) -> Model {
    let mut m = Model {
        sections: vec![Section::elastic("s", E, 1e-3, i)],
        ..Model::default()
    };
    let mut joint = Node::free("M", [2.0, 0.0, 1.501]);
    joint.spring = [1e3, 1e3, 1e3, 0.0, 0.0, 0.0];
    m.nodes = vec![
        Node::pinned("A", [0.0, 0.0, 0.0]),
        Node::pinned("B", [4.0, 0.0, 0.0]),
        Node::pinned("C", [4.0, 0.0, 3.0]),
        Node::pinned("D", [0.0, 0.0, 3.0]),
        joint,
    ];
    for (id, i) in [("AM", 0), ("MC", 2), ("BM", 1), ("MD", 3)] {
        m.members.push(Member::new(id, kind, i, 4, 0));
    }
    m
}

fn kinked_loads() -> Vec<NodalLoad> {
    vec![force(4, [H, 0.0, 0.0])]
}

/// The kinked joint, switched by ties alone, goes round (a cycle of the
/// halves of AC). [`solve`] switches the ties that go round member by
/// member, and the tie BD, whose half BM is then compressed while the
/// tie end to end is not, too: it settles without keeping any member
/// active. M is a real joint (loaded, sprung): pulled along x, it is held
/// by the stretched halves AM and MD; the shortened MC and BM are slack.
/// By the method of joints at M (AM along (0.8, 0.6), MD along
/// (0.8, −0.6), the spring nearly nothing): 2 · 0.8 N = H, N = H/1.6 =
/// 6.25 kN in each, within the 1 mm kink (0.1 %).
#[test]
fn the_formerly_cycling_kinked_joint_settles() {
    let m = kinked_joint(MemberKind::Tension, 0.0);
    assert_eq!(crate::solve::chains(&m), [vec![0, 1], vec![2, 3]]);
    // The iterations alone, by ties: a cycle of the halves of AC.
    let loads = Loads {
        nodal: kinked_loads(),
        self_weight: 0.0,
    };
    let ties = crate::solve::chains(&m);
    let e = settle(&m, false, |_, inactive| {
        let s = solve_stabilised(&m, &loads, &first_order(), &default_budget(), inactive, &[])?;
        let (next, shortening) = next_active_set(&m, &s, inactive, &ties, &[]);
        Ok((s, next, shortening))
    })
    .unwrap_err();
    assert!(
        matches!(&e, FeError::Unsettled { cycle: true, indices, .. } if indices == &[0, 1]),
        "{e}"
    );
    for options in [first_order(), Options::default()] {
        let s = solved(&m, kinked_loads(), &options);
        assert!(s.kept_active.is_empty(), "{s:?}");
        assert!(kept_active_note(&m, &s).is_none());
        for id in ["AM", "MD"] {
            let n = axial(&m, &s, id);
            assert!((n / (H / 1.6) - 1.0).abs() < 1e-3, "{id}: {n}");
        }
        for id in ["MC", "BM"] {
            let f = &s.members[index(&m, id)];
            assert!(f.slack && !f.bowed && f.axial == 0.0, "{id}: {f:?}");
        }
        // Equilibrium: the supports and the spring take the load, the
        // spring's −k·u reported as M's reaction.
        let sum: f64 = s.reactions.iter().map(|r| r.force[0]).sum();
        assert!(close(sum, -H), "{sum}");
        let at_m = s.reactions.iter().find(|r| r.node == 4).unwrap();
        let spring = -1e3 * s.displacements[4][0];
        assert!(close(at_m.force[0], spring), "{at_m:?} {spring}");
        assert!(s.passes > 2);
    }
}

/// The kinked joint with a second moment: MC and BM, shortened beyond
/// their buckling load N_cr = π²EI/L² (I = 0.1 cm⁴, L = 2.5 m, pinned
/// ends: 332 N), are
/// bowed and carry it, pushing M away from C and from B: (−1.6 N_cr, 0)
/// on M. AM and MD then take 2 · 0.8 N = H − 1.6 N_cr: N = H/1.6 − N_cr.
#[test]
fn the_kinked_joint_with_bending_stiffness_bows_its_shortened_halves() {
    let i = 1e-9;
    let m = kinked_joint(MemberKind::Tension, i);
    for options in [first_order(), Options::default()] {
        let s = solved(&m, kinked_loads(), &options);
        assert!(s.kept_active.is_empty(), "{s:?}");
        for id in ["MC", "BM"] {
            let k = index(&m, id);
            let ncr = std::f64::consts::PI.powi(2) * E * i / m.length(&m.members[k]).powi(2);
            let f = &s.members[k];
            assert!(f.bowed && !f.slack && f.axial == -ncr, "{id}: {f:?}");
        }
        let ncr = std::f64::consts::PI.powi(2) * E * i / 2.5f64.powi(2);
        for id in ["AM", "MD"] {
            let n = axial(&m, &s, id);
            assert!((n / (H / 1.6 - ncr) - 1.0).abs() < 1e-3, "{id}: {n}");
        }
        let sum: f64 = s.reactions.iter().map(|r| r.force[0]).sum();
        assert!(close(sum, -H), "{sum}");
    }
}

/// The X panel with diagonals of a second moment I = 1 cm⁴ (E = 210 GPa,
/// 5 m long): BD, shortened, carries its buckling load
/// N_cr = π²EI/L² = π² · 2100 N·m² / 25 m² = 829.05 N and no more. By the
/// method of joints, with BD's compression pushing D away from B
/// ((−0.8, 0.6) N_cr on D) and B away from D: AC = 5H/4 − N_cr, the top
/// DC = −H + 0.8 N_cr, the leg BC = −3H/4 + 0.6 N_cr, the leg AD
/// +0.6 N_cr (in tension: BD lifts D). First order exact; with P-Delta
/// close to it.
#[test]
fn x_panel_compressed_diagonal_carries_its_buckling_load() {
    let mut m = panel(MemberKind::Tension, false);
    m.sections[0].i_min = 1e-8;
    let ncr = std::f64::consts::PI.powi(2) * E * 1e-8 / 25.0;
    assert_eq!(crate::solve::buckling_loads(&m)[index(&m, "BD")], ncr);
    // Only tension-only members have one.
    assert_eq!(crate::solve::buckling_loads(&m)[index(&m, "AD")], 0.0);
    let s = solved(&m, vec![force(3, [H, 0.0, 0.0])], &first_order());
    let bd = &s.members[index(&m, "BD")];
    assert!(bd.bowed && !bd.slack && bd.axial == -ncr, "{bd:?}");
    assert_eq!((bd.end_forces[0], bd.end_forces[6]), (ncr, -ncr));
    assert!(close(axial(&m, &s, "AC"), 1.25 * H - ncr), "{s:?}");
    assert!(close(axial(&m, &s, "DC"), -H + 0.8 * ncr));
    assert!(close(axial(&m, &s, "BC"), -0.75 * H + 0.6 * ncr));
    assert!(close(axial(&m, &s, "AD"), 0.6 * ncr));
    assert_eq!(s.passes, 2);
    let sum: f64 = s.reactions.iter().map(|r| r.force[0]).sum();
    assert!(close(sum, -H), "{sum}");
    assert_eq!(
        tension_only_note(&m, &s).unwrap(),
        "tension-only members: 0 of 2 slack, 1 bowed (2 solve(s)); bowed (carry their \
         buckling load): BD 0.83 kN"
    );
    // Its usage: against the squash load (buckling is in the solve).
    let mut steel = m.clone();
    steel.sections[0].yield_strength = Some(355e6);
    let u = member_usage(&steel, &s, 1.0);
    let ub = &u[index(&m, "BD")];
    assert_eq!(
        ub.governs,
        "bowed: carries its buckling load 0.83 kN — compression 0.8 kN of 355.0 kN"
    );
    assert!((ub.usage.unwrap() - ncr / 355e3).abs() < 1e-12);
    // Without a capacity: no usage, but it says so.
    let u = member_usage(&m, &s, 1.0);
    let ub = &u[index(&m, "BD")];
    assert_eq!(
        (ub.usage, ub.governs.as_str()),
        (None, "bowed: carries its buckling load 0.83 kN")
    );
    // With P-Delta: still bowed, close to first order.
    let p = solved(&m, vec![force(3, [H, 0.0, 0.0])], &Options::default());
    assert!(p.members[index(&m, "BD")].bowed);
    assert_eq!(axial(&m, &p, "BD"), -ncr);
    assert!((axial(&m, &p, "AC") / (1.25 * H - ncr) - 1.0).abs() < 1e-2);
    let sum: f64 = p.reactions.iter().map(|r| r.force[0]).sum();
    assert!((sum + H).abs() < 1e-6 * H, "{sum}");
}

/// A tension-only member compressed less than its buckling load is a
/// truss: the X panel with stocky diagonals (I = 1000 cm⁴: N_cr = 829 kN
/// against about 6 kN) solves exactly as with truss diagonals, both
/// carrying, in one solve.
#[test]
fn compression_below_the_buckling_load_is_carried_as_a_truss() {
    let mut m = panel(MemberKind::Tension, false);
    m.sections[0].i_min = 1e-5;
    let mut t = panel(MemberKind::Truss, false);
    t.sections[0].i_min = 1e-5;
    for options in [first_order(), Options::default()] {
        let s = solved(&m, vec![force(3, [H, 0.0, 0.0])], &options);
        let r = solved(&t, vec![force(3, [H, 0.0, 0.0])], &options);
        assert!(axial(&m, &s, "BD") < -0.1 * H, "{s:?}");
        assert_eq!(s.members, r.members);
        assert_eq!(s.displacements, r.displacements);
        assert_eq!(s.passes, if options.nonlinear { 2 } else { 1 });
    }
}

/// The runs of the active-set iterations ([`crate::solve::rounds`]): a
/// tie going round is split into its members and run again; one hiding
/// compression too; single members going round are the last resort, kept
/// active as trusses (and a model left without tension-only members is
/// solved once).
#[test]
fn runs_split_ties_then_keep_members_as_the_last_resort() {
    let m = kinked_joint(MemberKind::Tension, 0.0);
    let unsettled = |indices: Vec<usize>| FeError::Unsettled {
        members: Vec::new(),
        flipping: indices.len(),
        indices,
        passes: 3,
        cycle: true,
    };
    // AM, MC go round as a tie; split, they settle.
    let mut seen: Vec<Vec<Vec<usize>>> = Vec::new();
    let (s, passes, kept) = rounds(
        &m,
        |_, ties| {
            seen.push(ties.to_vec());
            if ties.len() == 2 {
                Err(unsettled(vec![0, 1]))
            } else {
                Ok(("settled", 2, Vec::new()))
            }
        },
        |_| Ok("once"),
    )
    .unwrap();
    assert_eq!((s, passes, kept), ("settled", 5, vec![]));
    assert_eq!(seen[1], [vec![0], vec![1], vec![2, 3]]);
    // BM hides compression in the tie BD: split.
    let mut seen: Vec<Vec<Vec<usize>>> = Vec::new();
    let (_, passes, _) = rounds(
        &m,
        |_, ties| {
            seen.push(ties.to_vec());
            Ok(((), 2, if seen.len() == 1 { vec![2] } else { vec![] }))
        },
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(passes, 4);
    assert_eq!(seen[1], [vec![0, 1], vec![2], vec![3]]);
    // Single members going round: kept active, made trusses; with all
    // of them kept, one solve of the model left.
    let mut kinds: Vec<Vec<MemberKind>> = Vec::new();
    let (s, passes, kept) = rounds(
        &m,
        |model, ties| {
            kinds.push(model.members.iter().map(|x| x.kind).collect());
            Err(unsettled(ties.iter().flatten().copied().collect()))
        },
        |model| {
            assert!(model.members.iter().all(|x| x.kind == MemberKind::Truss));
            Ok("once")
        },
    )
    .unwrap();
    // Two ties split (3 solves), then all four kept (3), one solve.
    assert_eq!((s, passes, kept), ("once", 7, vec![0, 1, 2, 3]));
    assert_eq!(kinds.len(), 2);
}
