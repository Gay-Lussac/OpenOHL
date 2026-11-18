// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The 3D clash check at joints, on invented joints: two diagonals on one
//! flange, an X-brace crossing, a plate through a member it does not
//! hold, faces lying on each other.

use crate::clash::{ClashPart, convex_parts};
use crate::joints::joint_details;
use crate::joints_tests::{TOWER, library};
use crate::{JointDetail, Problem};

fn joint(text: &str, id: &str) -> JointDetail {
    let d = joint_details(text, &library(), &Default::default()).unwrap();
    assert!(d.level3, "{:?}", d.notes);
    d.joint(id).unwrap().clone()
}

fn pair(j: &JointDetail, a: &str, b: &str) -> Option<f64> {
    let (pa, pb) = (ClashPart::Member(a.into()), ClashPart::Member(b.into()));
    j.clashes
        .iter()
        .find(|c| (c.a == pa && c.b == pb) || (c.a == pb && c.b == pa))
        .map(|c| c.depth)
}

/// A second diagonal to N2 in the same face (y = 1 m), from above.
const D2: &str = r#"
[[node]]
id = "N5"
point = ["4 m", "1 m", "10 m"]
support = "fixed"

[[member]]
id = "D2"
nodes = ["N5", "N2"]
section = "UA-83x52x6"
design = { class = "bracing" }
"#;

fn cuts(d1: &str, d2: &str) -> String {
    format!(
        r#"
[[connection]]
member = "D1"
joint = "N2"
cut = "{d1}"

[[connection]]
member = "D2"
joint = "N2"
cut = "{d2}"
"#
    )
}

#[test]
fn an_angle_is_two_convex_pieces() {
    let l = [
        [0.0, 0.0],
        [0.08, 0.0],
        [0.08, 0.008],
        [0.008, 0.008],
        [0.008, 0.06],
        [0.0, 0.06],
    ];
    let parts = convex_parts(&l);
    assert_eq!(parts.len(), 2, "{parts:?}");
    // Every corner used, the areas adding up to the angle's.
    let area = |ix: &Vec<usize>| -> f64 {
        (0..ix.len())
            .map(|k| {
                let (a, b) = (l[ix[k]], l[ix[(k + 1) % ix.len()]]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>()
            .abs()
            / 2.0
    };
    let total: f64 = parts.iter().map(area).sum();
    let want = 0.08 * 0.008 + 0.008 * 0.052;
    assert!((total - want).abs() < 1e-12, "{total} {want}");
}

#[test]
fn two_diagonals_on_one_flange_clash_until_pulled_back() {
    // Both stop at the work point: their legs bolted lie on the same
    // flange of the leg, over each other.
    let j = joint(&format!("{TOWER}{D2}{}", cuts("0 mm", "0 mm")), "N2");
    let deep = pair(&j, "D1", "D2").expect("D1 and D2 clash");
    assert!(deep > 0.005, "{deep}");
    for m in ["D1", "D2"] {
        let a = j.arrivals.iter().find(|a| a.member == m).unwrap();
        assert!(a.misfit, "{m} drawn red");
        assert!(
            a.problems
                .iter()
                .any(|(p, w)| matches!(p, Problem::Clash { .. }) && w.contains("into each other")),
            "{:?}",
            a.problems
        );
    }
    // Pulled back far enough: apart.
    let j = joint(&format!("{TOWER}{D2}{}", cuts("250 mm", "250 mm")), "N2");
    assert_eq!(pair(&j, "D1", "D2"), None, "{:?}", j.clashes);
    // The leg is never in a clash with what bolts onto it.
    assert!(
        j.clashes.iter().all(
            |c| !matches!(&c.a, ClashPart::Member(m) if m.starts_with('L'))
                && !matches!(&c.b, ClashPart::Member(m) if m.starts_with('L'))
        ),
        "{:?}",
        j.clashes
    );
}

#[test]
fn faces_lying_on_each_other_touch_but_do_not_clash() {
    // The diagonal seated on the leg's flange, its end worked out: no
    // clash at all at N2 (the leg and the diagonal touch).
    let j = joint(TOWER, "N2");
    assert!(j.clashes.is_empty(), "{:?}", j.clashes);
    assert!(j.arrivals.iter().all(|a| !a.misfit));
}

/// An X brace: two diagonals crossing at C in the face y = 0, each in
/// two members running on through C as one part.
const X: &str = r#"id = "X"
name = "x"
kind = "lattice"

[[node]]
id = "A"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "B"
point = ["4 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "C"
point = ["2 m", "0 m", "2 m"]

[[node]]
id = "E"
point = ["4 m", "0 m", "4 m"]
support = "fixed"

[[node]]
id = "F"
point = ["0 m", "0 m", "4 m"]
support = "fixed"

[[member]]
id = "X1a"
nodes = ["A", "C"]
section = "EA-61x7"
design = { class = "bracing" }

[[member]]
id = "X1b"
nodes = ["C", "E"]
section = "EA-61x7"
design = { class = "bracing" }

[[member]]
id = "X2a"
nodes = ["B", "C"]
section = "EA-61x7"
design = { class = "bracing" }

[[member]]
id = "X2b"
nodes = ["C", "F"]
section = "EA-61x7"
design = { class = "bracing" }

[[connection]]
member = "X1a"
joint = "C"
kind = "continuous"
onto = "X1b"
"#;

#[test]
fn an_x_brace_crossing_clashes_unless_one_bolts_onto_the_other() {
    // Both diagonals run through C as one part, neither bolted to the
    // other: their steel crosses at C.
    let both = format!(
        "{X}\n[[connection]]\nmember = \"X2a\"\njoint = \"C\"\nkind = \"continuous\"\nonto = \
         \"X2b\"\n"
    );
    let j = joint(&both, "C");
    let crossing = j
        .clashes
        .iter()
        .filter(|c| {
            let m = |p: &ClashPart| p.member().map(|m| m.starts_with("X1")) == Some(true);
            let n = |p: &ClashPart| p.member().map(|m| m.starts_with("X2")) == Some(true);
            (m(&c.a) && n(&c.b)) || (n(&c.a) && m(&c.b))
        })
        .count();
    assert!(crossing >= 1, "{:?}", j.clashes);
    // X2 bolted onto X1 on both sides (one bolt each): it lies on X1's
    // leg, no clash between them.
    let bolted = format!(
        "{X}\n[[connection]]\nmember = \"X2a\"\njoint = \"C\"\nonto = \"X1a\"\nbolts = {{ count \
         = 1, diameter = \"16 mm\" }}\ncut = \"0 mm\"\n\n[[connection]]\nmember = \"X2b\"\njoint \
         = \"C\"\nonto = \"X1b\"\nbolts = {{ count = 1, diameter = \"16 mm\" }}\ncut = \"0 \
         mm\"\n"
    );
    let j = joint(&bolted, "C");
    for (a, b) in [("X1a", "X2a"), ("X1b", "X2b")] {
        assert_eq!(pair(&j, a, b), None, "{a} {b}: {:?}", j.clashes);
    }
}

#[test]
fn a_plate_through_a_member_it_does_not_hold() {
    // D1 onto a plate on the leg; D2 bolted onto the leg directly, its
    // end at the work point, where the plate is.
    let text = format!(
        r#"{TOWER}{D2}
[[plate]]
id = "G1"
joint = "N2"
thickness = "10 mm"
outline = [["0 m", "1 m", "4.85 m"], ["0.3 m", "1 m", "4.85 m"], ["0.3 m", "1 m", "5.15 m"], ["0 m", "1 m", "5.15 m"]]

[[connection]]
member = "D1"
joint = "N2"
onto = "G1"
bolts = {{ count = 2, diameter = "16 mm" }}

[[connection]]
member = "D2"
joint = "N2"
cut = "0 mm"
"#
    );
    let j = joint(&text, "N2");
    let through = j
        .clashes
        .iter()
        .find(|c| c.involves_plate("G1") && c.other_than("D2").is_some());
    assert!(through.is_some(), "{:?}", j.clashes);
    // Not with the member bolted onto it.
    assert!(
        !j.clashes
            .iter()
            .any(|c| c.involves_plate("G1") && c.a.member() == Some("D1")
                || c.involves_plate("G1") && c.b.member() == Some("D1")),
        "{:?}",
        j.clashes
    );
    let g1 = j.plates.iter().find(|p| p.id == "G1").unwrap();
    assert!(g1.notes.iter().any(|n| n.contains("into each other")));
}
