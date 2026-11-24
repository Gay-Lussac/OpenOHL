// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! End releases and bolt slip (stage 4.4 of the joint details, options of
//! the analysis, 2026-11-23) on invented models against closed forms.

use crate::joint_stiffness::{SLIP_TOLERANCE, apply, note};
use crate::{Loads, Model, NodalLoad, Options, Solution, default_budget, solve};

const E: f64 = 210e9;
const AREA: f64 = 1e-3;
const I: f64 = 2e-6;

fn first_order(model: &Model, loads: Vec<NodalLoad>) -> Solution {
    let options = Options {
        nonlinear: false,
        ..Options::default()
    };
    let loads = Loads {
        nodal: loads,
        self_weight: 0.0,
        ..Loads::default()
    };
    solve(model, &loads, &options, &default_budget()).unwrap()
}

fn load(node: usize, force: [f64; 3]) -> NodalLoad {
    NodalLoad {
        node,
        force,
        moment: [0.0; 3],
    }
}

/// A beam a–b–c along x, 2 × 2 m, fixed at a and c, two frames of equal
/// stiffness; `bolts` the design data of the members (sections `s1`,
/// `s2`; `same`: one section for both).
fn beam(bolts: &str, same: bool) -> Model {
    let second = if same { "s1" } else { "s2" };
    Model::parse(&format!(
        r#"
[analysis]
nonlinear = false
self_weight = false
capacity = "simple"
[[material]]
id = "steel"
elasticity = "210 GPa"
[[section]]
id = "s1"
material = "steel"
area = "1000 mm2"
iy = "200 cm4"
iz = "200 cm4"
[[section]]
id = "s2"
material = "steel"
area = "1000 mm2"
iy = "200 cm4"
iz = "200 cm4"
[[node]]
id = "a"
point = ["0 m", "0 m", "0 m"]
support = "fixed"
[[node]]
id = "b"
point = ["2 m", "0 m", "0 m"]
[[node]]
id = "c"
point = ["4 m", "0 m", "0 m"]
support = "fixed"
[[member]]
id = "ab"
nodes = ["a", "b"]
section = "s1"
kind = "frame"
[member.design]
bolts = {{ {bolts} }}
[[member]]
id = "bc"
nodes = ["b", "c"]
section = "{second}"
kind = "frame"
"#
    ))
    .unwrap()
}

/// One bolt: the end pinned. Member ab has one bolt at each end: pinned
/// at a (a fixed support: the member's end turns on it) and at b, where
/// bc (fixed at c) holds the joint — ab a two-hinged bar with no bending,
/// bc a cantilever carrying P: δ = P L³ / (3 E I); without the option the
/// fixed–fixed beam's P (2L)³ / (192 E I).
#[test]
fn one_bolt_pins_the_end() {
    let p = 10e3;
    let l: f64 = 2.0;
    let mut m = beam("diameter = \"16 mm\", count = 1", false);
    let off = first_order(&m, vec![load(1, [0.0, 0.0, -p])]);
    let rigid = p * (2.0 * l).powi(3) / (192.0 * E * I);
    assert!((off.displacements[1][2] + rigid).abs() < 1e-9 * rigid.max(1.0));
    m.analysis.end_releases = true;
    let sum = apply(&mut m);
    assert_eq!(sum.pinned, 2);
    assert!(sum.kept_rigid.is_empty());
    assert!(m.members[0].releases[1].both());
    let on = first_order(&m, vec![load(1, [0.0, 0.0, -p])]);
    // ab pinned at both ends: a two-hinged bar, no bending; bc a
    // cantilever from c carries it all: P L³ / (3 E I).
    let pinned = p * l.powi(3) / (3.0 * E * I);
    assert!(
        (on.displacements[1][2] + pinned).abs() < 1e-9 * pinned,
        "{} vs {pinned}",
        on.displacements[1][2]
    );
    // No moment at a released end.
    let f = &on.members[0].end_forces;
    assert!(f[4].abs() < 1e-6 && f[10].abs() < 1e-6, "{f:?}");
    assert!(note(&m).unwrap().contains("2 frame end(s) pinned"));
}

/// The same section in line at b: the end runs on through the joint —
/// not released there (only at a).
#[test]
fn same_section_in_line_runs_on() {
    let mut m = beam("diameter = \"16 mm\", count = 1", true);
    m.analysis.end_releases = true;
    let sum = apply(&mut m);
    assert_eq!(sum.pinned, 1);
    assert_eq!(sum.running_on, 1);
    assert!(m.members[0].releases[0].both() && m.members[0].releases[1].is_none());
}

/// One row of three bolts in the long leg: released about the leg's own
/// direction only (y for the long leg of a section without a profile),
/// the end still held in the leg's plane.
#[test]
fn one_row_frees_out_of_the_leg_plane() {
    let mut m = beam("diameter = \"16 mm\", count = 3, holes = 1", false);
    m.members[0].design.connected_leg = Some(crate::design::ConnectedLeg::Long);
    m.analysis.end_releases = true;
    let sum = apply(&mut m);
    assert_eq!((sum.pinned, sum.one_row), (0, 2));
    let r = m.members[0].releases[1];
    assert!(r.about_y && !r.about_z);
    // Two rows: held.
    let mut two = beam("diameter = \"16 mm\", count = 4, holes = 2", false);
    two.members[0].design.connected_leg = Some(crate::design::ConnectedLeg::Long);
    two.analysis.end_releases = true;
    assert_eq!(apply(&mut two).held, 2);
    assert!(two.members[0].releases.iter().all(|r| r.is_none()));
}

/// At b, ab bolted by one row (free about its y there) and bc by one
/// bolt (a bar): nothing would hold b against turning about ab's y while
/// ab still bends there — the stiffer released end at b is kept rigid,
/// said. Both pinned bars instead: b a bars' joint, nothing kept.
#[test]
fn a_joint_nothing_holds_keeps_an_end_rigid() {
    let mut m = beam("diameter = \"16 mm\", count = 3, holes = 1", false);
    m.members[0].design.connected_leg = Some(crate::design::ConnectedLeg::Long);
    let mut one = beam("diameter = \"16 mm\", count = 1", false);
    m.members[1].design = one.members[0].design.clone();
    m.analysis.end_releases = true;
    let sum = apply(&mut m);
    assert_eq!(sum.kept_rigid, vec!["b".to_owned()]);
    let at_b = [m.members[0].releases[1], m.members[1].releases[0]];
    assert_eq!(at_b.iter().filter(|r| r.is_none()).count(), 1);
    // ab's y is the structure's z (its reference): it bends about it no
    // more, so b is held along the structure's y (else a mechanism: bc
    // turning about its pin at c), loaded down.
    m.nodes[1].fixed[1] = true;
    let s = first_order(&m, vec![load(1, [0.0, 0.0, -10e3])]);
    assert!(s.displacements[1][2] < 0.0);
    // Both one bolt: two bars meeting at b, no rotations there.
    one.members[1].design = one.members[0].design.clone();
    one.analysis.end_releases = true;
    let sum = apply(&mut one);
    assert!(sum.kept_rigid.is_empty() && sum.pinned == 4);
    assert!(!one.members[0].is_frame() && !one.members[1].is_frame());
}

/// A bar a–b–c along x, 1 m + 1 m, pinned at a and c, b held across;
/// ab bolted (16 mm in 17.5 mm holes: 1.5 mm at each end, 3 mm in all),
/// bc not.
fn bars() -> Model {
    Model::parse(
        r#"
[analysis]
nonlinear = false
self_weight = false
capacity = "simple"
[[material]]
id = "steel"
elasticity = "210 GPa"
[[section]]
id = "s"
material = "steel"
area = "1000 mm2"
iy = "200 cm4"
iz = "200 cm4"
[[section]]
id = "t"
material = "steel"
area = "1000 mm2"
iy = "200 cm4"
iz = "200 cm4"
[[node]]
id = "a"
point = ["0 m", "0 m", "0 m"]
support = "pinned"
[[node]]
id = "b"
point = ["1 m", "0 m", "0 m"]
support = ["y", "z"]
[[node]]
id = "c"
point = ["2 m", "0 m", "0 m"]
support = "pinned"
[[member]]
id = "ab"
nodes = ["a", "b"]
section = "s"
[member.design]
bolts = { diameter = "16 mm", hole = "17.5 mm", count = 2 }
[[member]]
id = "bc"
nodes = ["b", "c"]
section = "t"
"#,
    )
    .unwrap()
}

/// Bolt slip, statically determinate: bc removed, b held along x by ab
/// alone — its pull P lengthens it by P L / EA plus the whole slip.
#[test]
fn slip_lengthens_a_determinate_bar_by_its_clearances() {
    let mut m = bars();
    m.members.truncate(1);
    m.nodes[2].fixed = [false; 6];
    m.nodes.truncate(2);
    m.analysis.bolt_slip = true;
    let sum = apply(&mut m);
    assert_eq!(sum.slipping, 1);
    assert!((m.members[0].slip - 3e-3).abs() < 1e-12);
    let p = 20e3;
    let s = first_order(&m, vec![load(1, [p, 0.0, 0.0])]);
    let want = p * 1.0 / (E * AREA) + 3e-3;
    assert!(
        (s.displacements[1][0] - want).abs() < 1e-9,
        "{}",
        s.displacements[1][0]
    );
    assert!((s.members[0].axial - p).abs() < 1e-6 * p);
    assert!(s.slip_settled && s.slip_passes >= 2);
}

/// Bolt slip, indeterminate: P at b shared by ab (slipping δ = 3 mm) and
/// bc. With P = 3kδ (k = EA/L): u = (P + kδ)/(2k) = 2δ, ab carries kδ in
/// tension, bc 2kδ in compression; without slip each half of P (to the
/// passes' tolerance).
#[test]
fn slip_sheds_force_to_the_stiffer_path() {
    let mut m = bars();
    let k = E * AREA / 1.0;
    let delta = 3e-3;
    let p = 3.0 * k * delta;
    let off = first_order(&m, vec![load(1, [p, 0.0, 0.0])]);
    assert!((off.members[0].axial - p / 2.0).abs() < 1e-6 * p);
    m.analysis.bolt_slip = true;
    apply(&mut m);
    let on = first_order(&m, vec![load(1, [p, 0.0, 0.0])]);
    let tol = 10.0 * SLIP_TOLERANCE;
    let u = 2.0 * delta;
    assert!(
        (on.displacements[1][0] - u).abs() < tol * delta,
        "{}",
        on.displacements[1][0]
    );
    assert!((on.members[0].axial - k * delta).abs() < tol * p);
    assert!((on.members[1].axial + k * u).abs() < tol * p);
    assert!(on.slip_settled);
    assert!(note(&m).unwrap().contains("1 member(s) slip"));
    assert!(crate::convergence_note(&on).unwrap().contains("bolt slip:"));
}

/// Both options off: nothing set, the solve as before bit for bit.
#[test]
fn options_off_change_nothing() {
    let mut m = bars();
    let before = first_order(&m, vec![load(1, [5e3, 0.0, 0.0])]);
    let sum = apply(&mut m);
    assert_eq!(sum.slipping + sum.pinned + sum.one_row, 0);
    assert!(m.joint_stiffness.is_none() && note(&m).is_none());
    let after = first_order(&m, vec![load(1, [5e3, 0.0, 0.0])]);
    assert_eq!(before, after);
    assert_eq!(after.slip_passes, 0);
}

/// Each member's usage the larger of with and without the options (stage
/// 4.4 follow-up, 2026-11-24): on `slip_sheds_force_to_the_stiffer_path`'s
/// bars (yield 200 MPa, so a pull N uses N / (200 MPa × 1000 mm²) = N /
/// 200 kN; P = 3kδ = 1890 kN) the slip relieves ab (kδ = 630 kN against P/2
/// = 945 kN without it: 3.15 against 4.725) and loads bc in compression
/// (2kδ = 1260 kN, Euler's N_cr = π² E I / L² = 4145 kN far above, its
/// yield 6.3 against 4.725): ab governed by the solve without the slip,
/// bc by the solve with it, one each.
#[test]
fn each_member_takes_the_larger_usage_of_with_and_without() {
    use crate::joint_stiffness::{effect, governs_without};
    let mut off_model = bars();
    for s in &mut off_model.sections {
        s.yield_strength = Some(200e6);
    }
    let k = E * AREA / 1.0;
    let p = 3.0 * k * 3e-3;
    let loads = vec![load(1, [p, 0.0, 0.0])];
    let off = first_order(&off_model, loads.clone());
    let mut on_model = off_model.clone();
    on_model.analysis.bolt_slip = true;
    apply(&mut on_model);
    let on = first_order(&on_model, loads);
    let u_off = crate::member_usage(&off_model, &off, 1.0);
    let u_on = crate::member_usage(&on_model, &on, 1.0);
    let usage =
        |u: &[crate::MemberUsage], k: usize| u.iter().find(|x| x.member == k).unwrap().usage;
    let tol = 10.0 * SLIP_TOLERANCE;
    assert!((usage(&u_off, 0).unwrap() - 945e3 / 200e3).abs() < tol);
    assert!((usage(&u_on, 0).unwrap() - 630e3 / 200e3).abs() < tol * 10.0);
    assert!((usage(&u_on, 1).unwrap() - 1260e3 / 200e3).abs() < tol * 10.0);
    // ab: without governs; bc: with governs; equal: with governs.
    assert!(governs_without(usage(&u_on, 0), usage(&u_off, 0)));
    assert!(!governs_without(usage(&u_on, 1), usage(&u_off, 1)));
    assert!(!governs_without(Some(0.5), Some(0.5)));
    assert!(governs_without(None, Some(0.1)));
    assert!(!governs_without(Some(0.1), None));
    assert!(governs_without(Some(0.9), Some(f64::NAN)));
    let d = std::time::Duration::ZERO;
    let e = effect(&on_model, (&on, &u_on, d), (&off, &u_off, d));
    assert_eq!(e.governed, (1, 1));
    assert!(e.words().contains(
        "1 member(s) governed by the solve with the options, 1 by the solve without them"
    ));
}

/// A buckling mode names the members it strains most (2026-11-24, the
/// joint options' taking back): a 10 m bar mast pinned at its foot, its top
/// held across (y) by a support and along x only by a 1 m bar of 1 mm² to a
/// pinned support (k = EA/L = 210 kN/m). It sways at P = k × 10 m = 2.1 MN
/// (the mast turning about its foot); under 3 MN (and 1 kN along x) the mode moves the top
/// most, and strains the short bar (u / 1 m) far more than the mast (no
/// length change as it turns): the bar comes first among the members
/// strained.
#[test]
fn a_buckling_mode_names_the_members_it_strains_most() {
    let m = Model::parse(
        r#"
[analysis]
nonlinear = true
self_weight = false
capacity = "simple"
[[material]]
id = "steel"
elasticity = "210 GPa"
[[section]]
id = "mast"
material = "steel"
area = "10000 mm2"
iy = "200 cm4"
iz = "200 cm4"
[[section]]
id = "tie"
material = "steel"
area = "1 mm2"
iy = "1 cm4"
iz = "1 cm4"
[[node]]
id = "foot"
point = ["0 m", "0 m", "0 m"]
support = "pinned"
[[node]]
id = "top"
point = ["0 m", "0 m", "10 m"]
support = ["y"]
[[node]]
id = "anchor"
point = ["1 m", "0 m", "10 m"]
support = "pinned"
[[member]]
id = "mast"
nodes = ["foot", "top"]
section = "mast"
kind = "truss"
[[member]]
id = "tie"
nodes = ["top", "anchor"]
section = "tie"
kind = "truss"
"#,
    )
    .unwrap();
    let loads = Loads {
        nodal: vec![load(1, [1e3, 0.0, -3e6])],
        self_weight: 0.0,
        ..Loads::default()
    };
    let e = solve(&m, &loads, &Options::default(), &default_budget()).unwrap_err();
    let crate::solve::FeError::Unstable {
        joints, straining, ..
    } = &e
    else {
        panic!("{e}")
    };
    assert_eq!(joints.first().map(String::as_str), Some("top"), "{e}");
    assert_eq!(
        straining.first().map(String::as_str),
        Some("tie"),
        "{straining:?}"
    );
    // Below the sway load it solves.
    let under = Loads {
        nodal: vec![load(1, [1e3, 0.0, -1e6])],
        self_weight: 0.0,
        ..Loads::default()
    };
    assert!(solve(&m, &under, &Options::default(), &default_budget()).is_ok());
}

/// Taking the options back at more than a tenth of a model's joints
/// refuses the case (O19 stage 4.4, the owner's answer of 2026-11-24):
/// the beam of [`a_joint_nothing_holds_keeps_an_end_rigid`] with b not
/// held across — released, ab and bc let b move along y (a mechanism), so
/// the releases are taken back at b: 1 of 3 joints (33 %) refuses the
/// case, said with the joints and why; the same beam beside nine more
/// joints (fixed, nothing on them: 1 of 12, 8 %) is solved, b taken back
/// and said.
#[test]
fn taking_back_beyond_a_tenth_of_the_joints_refuses_the_case() {
    let model = || {
        let mut m = beam("diameter = \"16 mm\", count = 3, holes = 1", false);
        m.members[0].design.connected_leg = Some(crate::design::ConnectedLeg::Long);
        let one = beam("diameter = \"16 mm\", count = 1", false);
        m.members[1].design = one.members[0].design.clone();
        m.analysis.end_releases = true;
        m
    };
    let mut m = model();
    apply(&mut m);
    let loads = Loads {
        nodal: vec![load(1, [0.0, 2e3, -10e3])],
        self_weight: 0.0,
        ..Loads::default()
    };
    let options = Options {
        nonlinear: false,
        ..Options::default()
    };
    let e = solve(&m, &loads, &options, &default_budget()).unwrap_err();
    let crate::solve::FeError::TakenBack {
        taken,
        joints,
        named,
        ..
    } = &e
    else {
        panic!("{e}")
    };
    assert_eq!((*taken, *joints), (1, 3), "{e}");
    assert_eq!(named, &vec!["b".to_owned()]);
    let words = e.to_string();
    for w in [
        "refused with the joint options",
        "1 of 3 joints (33 %, more than the limit of 10 %)",
        "at b",
        "solve this case without the joint options",
    ] {
        assert!(words.contains(w), "{w}: {words}");
    }
    // Nine more joints: 1 of 12 is within the limit — solved, said.
    let mut m = model();
    for k in 0..9 {
        let mut n = m.nodes[0].clone();
        n.id = format!("x{k}");
        n.point = [0.0, 1.0 + f64::from(k), 0.0];
        m.nodes.push(n);
    }
    apply(&mut m);
    let s = solve(&m, &loads, &options, &default_budget()).unwrap();
    assert_eq!(s.taken_back, vec!["b".to_owned()]);
    assert!(
        crate::joint_stiffness::case_note(&s)
            .unwrap()
            .contains("taken back at the members meeting 1 joint(s) of 12 (8 %)")
    );
}
