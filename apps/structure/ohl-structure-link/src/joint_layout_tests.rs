// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plane layout engine on invented joints: a K-brace, an X-brace, a
//! shallow redundant, two faces at a corner, a built-up leg; the same
//! input gives the same plan, and a plan written is a plan kept.

use crate::JointDetail;
use crate::joint_layout::mm_text;
use crate::joint_layout::{BoltArea, JointPlan, LayoutSettings, Onto, plan_joint};
use crate::joints::joint_details;
use crate::joints_tests::library;

/// The settings of the tests: the gap and limits to start with, the bolt
/// picked a 16 mm one.
fn settings() -> LayoutSettings {
    LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    }
}

fn joint(text: &str, id: &str) -> JointDetail {
    let d = joint_details(text, &library(), &Default::default()).unwrap();
    assert!(d.level3, "{:?}", d.notes);
    d.joint(id).unwrap().clone()
}

fn plan(text: &str, id: &str) -> JointPlan {
    plan_joint(&joint(text, id), &settings(), &|_, _| None)
}

/// `text` with `p` written as connections given (and its plates).
fn written(text: &str, p: &JointPlan) -> String {
    let mut out = text.to_owned();
    for (k, pl) in p.plates.iter().enumerate() {
        out.push_str(&format!(
            "\n[[plate]]\nid = \"LP{k}\"\njoint = \"{}\"\nthickness = \"{}\"\n",
            p.joint,
            mm_text(pl.thickness)
        ));
    }
    for e in &p.ends {
        out.push_str(&format!(
            "\n[[connection]]\nmember = \"{}\"\njoint = \"{}\"\n",
            e.member, p.joint
        ));
        match &e.onto {
            Onto::Member(m) => out.push_str(&format!("onto = \"{m}\"\n")),
            Onto::Plate(k) => out.push_str(&format!("onto = \"LP{k}\"\n")),
        }
        if let Some(c) = e.cut {
            out.push_str(&format!("cut = \"{}\"\n", mm_text(c)));
        }
        if let Some(s) = e.shift {
            out.push_str(&format!("shift = \"{}\"\n", mm_text(s)));
        }
        if let Some(b) = &e.bolts {
            out.push_str(&format!(
                "bolts = {{ count = {}, diameter = \"{}\" }}\ngauge_lines = {}\n",
                b.count,
                mm_text(b.diameter),
                b.lines
            ));
        }
    }
    // The ends drawn with their gauge line through the work point: the
    // record's mark (as the layout run writes it).
    let marks: Vec<String> = p
        .ends
        .iter()
        .filter(|e| e.on_gauge)
        .map(|e| format!("[\"{}\", \"{}\", \"gauge\"]", e.member, p.joint))
        .collect();
    if !marks.is_empty() {
        out.push_str(&format!(
            "\n[[generator]]\nid = \"joint-layout\"\nkind = \"joint_layout\"\nends = [{}]\n",
            marks.join(", ")
        ));
    }
    out
}

/// The clashes of joint `id` among the members laid out (and their bolts
/// and plates).
fn clashes(text: &str, id: &str, p: &JointPlan) -> Vec<String> {
    let j = joint(text, id);
    let laid: Vec<&str> = p.ends.iter().map(|e| e.member.as_str()).collect();
    j.clashes
        .iter()
        .filter(|c| {
            [&c.a, &c.b].iter().any(|x| match x {
                crate::ClashPart::Plate(id) => id.starts_with("LP"),
                other => other.member().is_some_and(|m| laid.contains(&m)),
            })
        })
        .map(|c| c.words())
        .collect()
}

/// A K-brace: two diagonals of one face meeting the leg at N2, one from
/// below, one from above.
pub(crate) const K: &str = r#"id = "K"
name = "k"
kind = "lattice"

[[node]]
id = "N1"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N2"
point = ["0 m", "0 m", "3 m"]

[[node]]
id = "N3"
point = ["0 m", "0 m", "6 m"]
support = "fixed"

[[node]]
id = "N4"
point = ["2.5 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N5"
point = ["2.5 m", "0 m", "6 m"]
support = "fixed"

[[member]]
id = "L1"
nodes = ["N1", "N2"]
section = "EA-61x7"
orientation = "x+y+"
design = { class = "leg" }

[[member]]
id = "L2"
nodes = ["N2", "N3"]
section = "EA-61x7"
orientation = "x+y+"
design = { class = "leg" }

[[member]]
id = "D1"
nodes = ["N4", "N2"]
section = "EA-61x7"
orientation = "y-z+"
design = { class = "bracing" }

[[member]]
id = "D2"
nodes = ["N5", "N2"]
section = "EA-61x7"
orientation = "z-y-"
design = { class = "bracing" }
"#;

#[test]
fn a_k_brace_is_laid_out_concentric_and_fits() {
    let p = plan(K, "N2");
    assert!(p.laid_out(), "{:?}", p.hand);
    let ends: Vec<&str> = p.ends.iter().map(|e| e.member.as_str()).collect();
    assert_eq!(ends, ["D1", "D2"], "{:?}", p.left);
    // Concentric: the gauge lines through the work point; each pulled
    // back (a cut), provisional bolts (none of their own).
    for e in &p.ends {
        assert!(e.off.abs() < 1e-9, "{e:?}");
        assert!(e.cut.unwrap() > 0.0, "{e:?}");
        assert!(e.bolts.is_some());
    }
    // The legs in line through the joint are left alone.
    assert!(
        p.left
            .iter()
            .any(|l| l.starts_with("L1") || l.starts_with("L2"))
    );
    let text = written(K, &p);
    assert!(
        clashes(&text, "N2", &p).is_empty(),
        "{:?}",
        clashes(&text, "N2", &p)
    );
}

#[test]
fn the_same_joint_gives_the_same_plan_and_a_plan_written_is_kept() {
    let a = plan(K, "N2");
    let b = plan(K, "N2");
    assert_eq!(a, b);
    // Written as given by hand, planned again: kept as it is (its cuts and
    // shifts are what fits; on a plate given by hand: left as it is),
    // nothing for a hand detail.
    let text = written(K, &a);
    let again = plan(&text, "N2");
    assert!(again.hand.is_empty(), "{:?}", again.hand);
    for y in &again.ends {
        assert_eq!(y.cut, None, "{y:?}");
    }
    for e in &a.ends {
        assert!(
            again.ends.iter().any(|y| y.member == e.member)
                || again.left.iter().any(|l| l.starts_with(&e.member)),
            "{again:?}"
        );
    }
}

/// An X brace: two diagonals crossing at C, X1 one part through it, X2
/// in two members bolted onto it.
pub(crate) const X: &str = r#"id = "X"
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
orientation = "z+y-"
design = { class = "bracing" }

[[member]]
id = "X1b"
nodes = ["C", "E"]
section = "EA-61x7"
orientation = "z+y-"
design = { class = "bracing" }

[[member]]
id = "X2a"
nodes = ["B", "C"]
section = "EA-61x7"
orientation = "y-z+"
design = { class = "crossing" }

[[member]]
id = "X2b"
nodes = ["C", "F"]
section = "EA-61x7"
orientation = "y-z+"
design = { class = "crossing" }

[[connection]]
member = "X1a"
joint = "C"
kind = "continuous"
onto = "X1b"
"#;

#[test]
fn an_x_brace_crossing_is_laid_out_on_the_through_member() {
    let p = plan(X, "C");
    assert!(p.laid_out(), "{:?} {:?}", p.hand, p.left);
    for e in &p.ends {
        assert!(e.member.starts_with("X2"), "{e:?}");
        let on = match &e.onto {
            Onto::Member(m) => m.clone(),
            Onto::Plate(k) => p.plates[*k].on.clone(),
        };
        assert!(on.starts_with("X1"), "{e:?}");
        assert!(e.off.abs() < 1e-9, "concentric: {e:?}");
    }
    let text = written(X, &p);
    assert!(
        clashes(&text, "C", &p).is_empty(),
        "{:?}",
        clashes(&text, "C", &p)
    );
}

/// A redundant nearly along the diagonal (3°) at N2.
const SHALLOW: &str = r#"
[[node]]
id = "N6"
point = ["2.5 m", "0 m", "0.13 m"]
support = "fixed"

[[member]]
id = "R1"
nodes = ["N6", "N2"]
section = "EA-61x7"
orientation = "y-z+"
design = { class = "redundant" }
"#;

#[test]
fn a_shallow_redundant_is_shifted_or_left_for_a_hand_detail_never_forced() {
    let text = format!("{K}{SHALLOW}");
    let p = plan(&text, "N2");
    if let Some(r1) = p.ends.iter().find(|e| e.member == "R1") {
        // Shifted off the work point: the gauge line moved.
        assert!(r1.off.abs() > 0.0, "{r1:?}");
        let written = written(&text, &p);
        assert!(clashes(&written, "N2", &p).is_empty());
    } else {
        assert!(
            p.hand
                .iter()
                .any(|h| h.contains("R1") && h.contains("within")),
            "{:?}",
            p.hand
        );
        // R1 left as it is, the rest laid out.
        assert_eq!(p.hand_members, ["R1"]);
    }
    // With no shift allowed and a short pull-back: a hand detail, with why;
    // R1 left as it is, the rest laid out.
    let s = LayoutSettings {
        shift_limit: 0.0,
        pull_back: 0.12,
        ..settings()
    };
    let q = plan_joint(&joint(&text, "N2"), &s, &|_, _| None);
    assert!(q.hand.iter().any(|h| h.contains("R1")), "{:?}", q.hand);
    assert_eq!(q.hand_members, ["R1"]);
    assert!(q.ends.iter().all(|e| e.member != "R1"));
}

/// Two faces at a corner: a diagonal in the face y = 0 and one in x = 0,
/// on the leg's two flanges.
const CORNER: &str = r#"
[[node]]
id = "N7"
point = ["0 m", "2.5 m", "0 m"]
support = "fixed"

[[member]]
id = "E1"
nodes = ["N7", "N2"]
section = "EA-61x7"
orientation = "z+x-"
design = { class = "bracing" }
"#;

/// The K brace without D2, with E1 in the other face (two faces at a
/// corner).
pub(crate) fn corner_text() -> String {
    format!(
        "{}{CORNER}",
        K.replace(
            "[[member]]\nid = \"D2\"\nnodes = [\"N5\", \"N2\"]\nsection = \"EA-61x7\"\norientation = \"z-y-\"\ndesign = { class = \"bracing\" }\n",
            "",
        )
    )
}

#[test]
fn two_faces_at_a_corner_are_two_planes() {
    let text = corner_text();
    let p = plan(&text, "N2");
    assert!(p.laid_out(), "{:?} {:?}", p.hand, p.left);
    let ends: Vec<&str> = p.ends.iter().map(|e| e.member.as_str()).collect();
    assert_eq!(ends, ["D1", "E1"]);
    // A plate per face where one is needed, never one for both.
    for pl in &p.plates {
        assert_eq!(pl.members.len(), 1, "{pl:?}");
    }
    let w = written(&text, &p);
    assert!(
        clashes(&w, "N2", &p).is_empty(),
        "{:?}",
        clashes(&w, "N2", &p)
    );
}

#[test]
fn a_built_up_leg_is_met_on_the_angle_facing_the_member_never_forced() {
    // One bolt each (their own): they land on the facing angle's flange.
    let text = K
        .replace(
            "section = \"EA-61x7\"\norientation = \"x+y+\"",
            "section = \"4X-61x7\"",
        )
        .replace(
            "design = { class = \"bracing\" }",
            "design = { class = \"bracing\", bolts = { diameter = \"12 mm\", count = 1 } }",
        );
    // On the inner face of the facing angle (as turned): clear of its other
    // leg, the bolt does not land on the flange, and a plate on a built-up
    // member is not laid out automatically; not laid out, said.
    let p = plan(&text, "N2");
    assert!(!p.laid_out());
    assert!(
        p.hand.iter().all(|h| h.contains("is built up")),
        "{:?}",
        p.hand
    );
    // The facing angle found (not the gap of the cross): its flange named.
    assert!(p.hand.iter().all(|h| h.contains("L2") || h.contains("L1")));
}

#[test]
fn bolts_off_the_flange_go_onto_a_plate_of_the_thickest_leg() {
    // Many bolts: they do not all land on the leg's narrow flange.
    let text = K.replacen(
        "design = { class = \"bracing\" }",
        "design = { class = \"bracing\", bolts = { diameter = \"16 mm\", count = 6 } }",
        1,
    );
    let p = plan(&text, "N2");
    assert!(p.laid_out(), "{:?}", p.hand);
    assert_eq!(p.plates.len(), 1, "{p:?}");
    let pl = &p.plates[0];
    assert!(pl.members.contains(&"D1".to_owned()) && pl.members.contains(&"D2".to_owned()));
    assert!((pl.thickness - 0.007).abs() < 1e-9, "{}", pl.thickness);
    assert!(p.ends.iter().all(|e| e.onto == Onto::Plate(0)));
    let w = written(&text, &p);
    assert!(
        clashes(&w, "N2", &p).is_empty(),
        "{:?}",
        clashes(&w, "N2", &p)
    );
}
