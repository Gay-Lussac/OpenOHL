// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Eccentric member ends (connection shifts, rigid end offsets; stage 4.4
//! of the joint details, 2026-11-02) on invented models, by hand: a
//! column C (a frame, fixed at its foot N1, 4 m high) and a member T from
//! its top N2 to N3, 3 m away along x, T shifted at N2 by e towards C
//! (down: its axis from (0, 0, 4 − e) to N3).

use ohl_sections::SectionLibrary;
use ohl_structures::bolts::BoltLibrary;

use crate::check::SHIFTED;
use crate::{
    Loads, Model, NodalLoad, Options, Solution, default_budget, eccentricity_note, member_usage,
    solve,
};

const E: f64 = 210e9;
const I: f64 = 2e-6;
/// The shift, m.
const SHIFT: f64 = 0.02;
/// The pull, N.
const P: f64 = 10e3;

/// The column and T (`kind`), N3 held by `n3` (a support line or none);
/// with `shift` the connection of T at N2 onto C shifted by it.
fn tower(kind: &str, n3: &str, shift: Option<f64>, nonlinear: bool) -> Model {
    let connection = shift.map_or(String::new(), |s| {
        format!(
            "[[connection]]\nmember = \"T\"\njoint = \"N2\"\nonto = \"C\"\nshift = \"{} mm\"\n",
            s * 1e3
        )
    });
    let text = format!(
        r#"
[analysis]
nonlinear = {nonlinear}
self_weight = false

[[material]]
id = "steel"
elasticity = "210 GPa"

[[section]]
id = "s"
material = "steel"
area = "1000 mm2"
iy = "200 cm4"
iz = "200 cm4"

[[node]]
id = "N1"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N2"
point = ["0 m", "0 m", "4 m"]

[[node]]
id = "N3"
point = ["3 m", "0 m", "4 m"]
{n3}

[[member]]
id = "C"
nodes = ["N1", "N2"]
section = "s"
kind = "frame"

[[member]]
id = "T"
nodes = ["N2", "N3"]
section = "s"
kind = "{kind}"
{connection}"#
    );
    Model::from_structure_type_with_bolts(
        &text,
        &SectionLibrary::default(),
        &BoltLibrary::default(),
        None,
    )
    .unwrap()
}

fn run(m: &Model, force: [f64; 3], nonlinear: bool) -> Solution {
    let loads = Loads {
        nodal: vec![NodalLoad {
            node: 2,
            force,
            moment: [0.0; 3],
        }],
        self_weight: 0.0,
        ..Default::default()
    };
    let options = Options {
        nonlinear,
        ..Options::default()
    };
    solve(m, &loads, &options, &default_budget()).unwrap()
}

/// The moment at C's top (its end at N2), N·m.
fn top_moment(s: &Solution) -> f64 {
    let e = s.members[0].end_forces;
    (e[10] * e[10] + e[11] * e[11]).sqrt()
}

/// The shift's offset: across T (along x), towards C (down from N2):
/// (0, 0, −e).
#[test]
fn the_shift_is_an_offset_towards_what_it_bolts_onto() {
    let m = tower("truss", "support = [\"y\", \"z\"]", Some(SHIFT), false);
    assert_eq!(m.members[1].offsets, [[0.0, 0.0, -SHIFT], [0.0; 3]]);
    let ends = m.eccentric_ends(1);
    assert_eq!(ends.len(), 1);
    assert!((ends[0].e - SHIFT).abs() < 1e-15);
    assert!(ends[0].carried, "a frame meets N2");
    // Without a shift no offset: the model as before.
    let plain = tower("truss", "support = [\"y\", \"z\"]", None, false);
    assert!(!plain.has_offsets());
}

/// A truss T pulled by P along x at N3 (held in y and z), shifted at N2
/// onto the frame leg C. Its axis from (0, 0, 4 − e) to (3, 0, 4): N =
/// P·L′/3 (L′ = √(9 + e²)), on N2 the force (P, 0, P·e/3) and the moment
/// r × F = (0, 0, −e) × (P, 0, P·e/3) = (0, −P·e, 0): C's top takes the
/// moment P·e = 200 N m (0 without the shift); its foot P·(4 − e) =
/// 39.8 kN m (40 without); N2 moves P·L³/3EI − P·e·L²/2EI along x.
#[test]
fn a_truss_shifted_onto_a_frame_leg_puts_n_e_into_the_leg() {
    let m = tower("truss", "support = [\"y\", \"z\"]", Some(SHIFT), false);
    let s = run(&m, [P, 0.0, 0.0], false);
    let l2 = (9.0 + SHIFT * SHIFT).sqrt();
    assert!((s.members[1].axial - P * l2 / 3.0).abs() < 1e-6 * P);
    assert!((top_moment(&s) - P * SHIFT).abs() < 1e-6 * P * SHIFT);
    let foot = s.reactions.iter().find(|r| r.node == 0).unwrap();
    assert!((foot.moment[1] + P * (4.0 - SHIFT)).abs() < 1e-6 * P);
    assert!((foot.force[2] + P * SHIFT / 3.0).abs() < 1e-6 * P);
    let u = P * 64.0 / (3.0 * E * I) - P * SHIFT * 16.0 / (2.0 * E * I);
    assert!((s.displacements[1][0] - u).abs() < 1e-9 * u.abs().max(1.0));
    // Said: where the moment went, per member and in the usage.
    let usage = member_usage(&m, &s, 1.0);
    assert!(usage[1].governs.contains(SHIFTED), "{}", usage[1].governs);
    assert!(
        usage[1]
            .governs
            .contains("N2 e 20 mm, 0.20 kN m to the joint's frame members"),
        "{}",
        usage[1].governs
    );
    let note = eccentricity_note(&m, &s).unwrap();
    assert!(note.contains("1 end(s) of 1 member(s)"), "{note}");
    assert!(!note.contains("LEFT OUT"), "{note}");
    // Without the shift: no moment at C's top, 4·P at its foot.
    let plain = tower("truss", "support = [\"y\", \"z\"]", None, false);
    let s0 = run(&plain, [P, 0.0, 0.0], false);
    assert!(top_moment(&s0) < 1e-9 * P);
    let foot0 = s0.reactions.iter().find(|r| r.node == 0).unwrap();
    assert!((foot0.moment[1] + 4.0 * P).abs() < 1e-6 * P);
    assert!(eccentricity_note(&plain, &s0).is_none());
}

/// T a frame, N3 free, pulled by P along T's own axis (from its offset
/// end at N2 to N3): T carries N = P and no moment; C's top takes N
/// times the distance of T's axis from N2, e·3/L′ (the frame's offset
/// end tied rigidly to the joint); C's foot the moment of the force,
/// 4·F_x − 3·F_z.
#[test]
fn a_frame_with_an_offset_end_gives_the_joint_n_e() {
    let m = tower("frame", "", Some(SHIFT), false);
    let l2 = (9.0 + SHIFT * SHIFT).sqrt();
    let f = [P * 3.0 / l2, 0.0, P * SHIFT / l2];
    let s = run(&m, f, false);
    assert!((s.members[1].axial - P).abs() < 1e-6 * P);
    let t = s.members[1].end_forces;
    for k in [4, 5, 10, 11] {
        assert!(t[k].abs() < 1e-6 * P * SHIFT, "T end moment {k}: {}", t[k]);
    }
    assert!((top_moment(&s) - P * SHIFT * 3.0 / l2).abs() < 1e-6 * P * SHIFT);
    let foot = s.reactions.iter().find(|r| r.node == 0).unwrap();
    assert!((foot.moment[1] + (4.0 * f[0] - 3.0 * f[2])).abs() < 1e-6 * P);
    let usage = member_usage(&m, &s, 1.0);
    assert!(
        usage[1]
            .governs
            .contains("in its end moments and the joint's frames"),
        "{}",
        usage[1].governs
    );
}

/// The same truss shifted at N3, where only it meets (N3 held in y and
/// z): the moment cannot be carried there — that offset left out (the
/// force through the work point), said, and the solve runs with the
/// results of no shift.
#[test]
fn onto_a_bar_only_joint_the_moment_is_left_out_and_said() {
    let text = |shift: bool| {
        let mut m = tower("truss", "support = [\"y\", \"z\"]", None, false);
        if shift {
            m.members[1].offsets[1] = [0.0, 0.0, -SHIFT];
        }
        m
    };
    let m = text(true);
    let ends = m.eccentric_ends(1);
    assert!(!ends[0].carried);
    let s = run(&m, [P, 0.0, 0.0], false);
    let s0 = run(&text(false), [P, 0.0, 0.0], false);
    assert_eq!(s.members, s0.members);
    assert_eq!(s.displacements, s0.displacements);
    let note = eccentricity_note(&m, &s).unwrap();
    assert!(note.contains("LEFT OUT at 1 end(s)"), "{note}");
    assert!(note.contains("T at N3 0.20 kN m"), "{note}");
    let usage = member_usage(&m, &s, 1.0);
    assert!(
        usage[1].governs.contains("LEFT OUT"),
        "{}",
        usage[1].governs
    );
}

/// P-Delta with the shift: converges, in balance, and close to the first
/// order (C's axial force is small): its top moment P·e within 1 %.
#[test]
fn p_delta_runs_with_offsets() {
    let m = tower("truss", "support = [\"y\", \"z\"]", Some(SHIFT), true);
    let s = run(&m, [P / 10.0, 0.0, 0.0], true);
    assert!(s.nonlinear);
    let sum: [f64; 3] = [0, 1, 2].map(|k| s.reactions.iter().map(|r| r.force[k]).sum());
    assert!((sum[0] + P / 10.0).abs() < 1e-6 * P);
    assert!(sum[1].abs() < 1e-6 * P && sum[2].abs() < 1e-6 * P);
    assert!((top_moment(&s) - P / 10.0 * SHIFT).abs() < 0.01 * P / 10.0 * SHIFT);
}

/// A tension-only T in tension: active, the truss's results (first order
/// and P-Delta) — the active set works between the offset ends.
#[test]
fn a_tension_only_member_with_an_offset_end() {
    for nonlinear in [false, true] {
        let truss = tower("truss", "support = [\"y\", \"z\"]", Some(SHIFT), nonlinear);
        let tension = tower(
            "tension",
            "support = [\"y\", \"z\"]",
            Some(SHIFT),
            nonlinear,
        );
        let a = run(&truss, [P / 10.0, 0.0, 0.0], nonlinear);
        let b = run(&tension, [P / 10.0, 0.0, 0.0], nonlinear);
        assert!(!b.members[1].slack);
        assert!((a.members[1].axial - b.members[1].axial).abs() < 1e-6 * P);
        assert!((top_moment(&a) - top_moment(&b)).abs() < 1e-6 * P * SHIFT);
    }
}
