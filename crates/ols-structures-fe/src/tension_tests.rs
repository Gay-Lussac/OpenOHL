// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Tension-only members on invented models: an X-braced panel whose
//! compressed diagonal goes slack (against the hand solution of the
//! panel with one diagonal), the load reversed, a crossing joint held
//! only by slack halves, P-Delta, the cycle and pass limits of the
//! active-set iterations, and models whose tension-only members stay
//! taut solving exactly as trusses.

use crate::solve::{TENSION_PASSES, settle};
use crate::{
    FeError, Loads, Member, MemberKind, Model, NodalLoad, Node, Options, Section, Solution,
    default_budget, member_usage, solve, tension_only_note,
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
        "format = \"ols-structure-model\"\nversion = 2\n{body}"
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
        "format = \"ols-structure-model\"\nversion = {}\n{body}",
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
    } = &e
    else {
        panic!("{e}");
    };
    assert!(*cycle && *passes == 3, "{e}");
    assert_eq!(
        (members.as_slice(), *flipping),
        (&["AC".to_owned(), "BD".to_owned()][..], 2)
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
