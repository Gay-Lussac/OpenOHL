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

/// The K brace without D2, and a short redundant R1 at N2 `deg` degrees
/// less steep than D1 (in its face), `len` long.
pub(crate) fn yielding(deg: f64, len: f64) -> String {
    let down = (3.0f64 / 2.5).atan().to_degrees() - deg;
    let (c, s) = (down.to_radians().cos(), down.to_radians().sin());
    format!(
        "{}\n[[node]]\nid = \"N8\"\npoint = [\"{:.4} m\", \"0 m\", \"{:.4} m\"]\nsupport = \"fixed\"\n\
         \n[[member]]\nid = \"R1\"\nnodes = [\"N8\", \"N2\"]\nsection = \"EA-61x7\"\n\
         orientation = \"y-z+\"\ndesign = {{ class = \"redundant\" }}\n",
        K.replace(
            "[[member]]\nid = \"D2\"\nnodes = [\"N5\", \"N2\"]\nsection = \"EA-61x7\"\norientation = \"z-y-\"\ndesign = { class = \"bracing\" }\n",
            "",
        ),
        len * c,
        3.0 - len * s
    )
}

/// `text`'s joint `id` planned with at most `limit` placings.
fn plan_limited(text: &str, id: &str, limit: usize) -> JointPlan {
    crate::joint_layout::plan_joint_budget(
        &joint(text, id),
        &settings(),
        &|_, _| None,
        &Default::default(),
        &Default::default(),
        limit,
    )
}

fn cut_of(p: &JointPlan, m: &str) -> Option<f64> {
    p.ends.iter().find(|e| e.member == m).and_then(|e| e.cut)
}

/// J1 (2026-11-19): the owner's order filled one member after the other
/// leaves the short redundant R1 out (D1 placed first takes the room near
/// the work point, R1 cannot be pulled back far enough within half its
/// length less its bolts); the search places R1 first and pulls D1 back
/// instead — both laid out, concentric, running into nothing.
#[test]
fn the_search_lays_out_what_one_after_the_other_leaves() {
    let text = yielding(20.0, 0.4);
    // One after the other (a single placing: the search's first path).
    let greedy = plan_limited(&text, "N2", 1);
    assert_eq!(greedy.hand_members, ["R1"], "{greedy:?}");
    assert!(
        greedy
            .hand
            .iter()
            .any(|h| h.contains("the search stopped at its limit of 1 tries")),
        "{:?}",
        greedy.hand
    );
    // Searched: both, D1 (the bigger class) pulled back past R1.
    let p = plan(&text, "N2");
    assert!(p.hand.is_empty(), "{:?}", p.hand);
    let (d1, r1) = (cut_of(&p, "D1").unwrap(), cut_of(&p, "R1").unwrap());
    assert!(d1 > r1, "{p:?}");
    assert!(d1 <= settings().pull_back + 1e-9, "{d1}");
    for e in &p.ends {
        assert!(e.off.abs() < 1e-9, "concentric: {e:?}");
    }
    let w = written(&text, &p);
    assert!(
        clashes(&w, "N2", &p).is_empty(),
        "{:?}",
        clashes(&w, "N2", &p)
    );
}

/// J1: the search is deterministic (the same joint, the same plan; ties
/// go to the owner's order), and a plan written and planned again is kept.
#[test]
fn the_search_gives_the_same_plan_and_keeps_a_plan_written() {
    for (deg, len) in [(20.0, 0.4), (25.0, 0.5), (30.0, 0.6)] {
        let text = yielding(deg, len);
        let a = plan(&text, "N2");
        let b = plan(&text, "N2");
        assert_eq!(a, b);
        let again = plan(&written(&text, &a), "N2");
        assert!(again.hand.is_empty(), "{:?}", again.hand);
        assert!(again.ends.iter().all(|e| e.cut.is_none()), "{again:?}");
    }
}

/// J1: a value given by hand is kept — D1's cut given, the search lays
/// R1 out round it and never changes it.
#[test]
fn the_search_keeps_a_cut_given_by_hand() {
    let text = format!(
        "{}\n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\ncut = \"250 mm\"\n",
        yielding(20.0, 0.4)
    );
    let p = plan(&text, "N2");
    assert!(p.hand.is_empty(), "{:?}", p.hand);
    let d1 = p.ends.iter().find(|e| e.member == "D1").unwrap();
    assert_eq!(d1.cut, None, "kept: {d1:?}");
    assert!(cut_of(&p, "R1").unwrap() < 0.25, "{p:?}");
}

/// J2 (2026-11-19): a member left for a hand detail for its own sake (R1,
/// too short for its bolts) keeps its place as it is: D1 is laid out
/// pulled back clear of it, and nothing laid out runs into it.
#[test]
fn a_member_left_by_hand_keeps_its_place() {
    let text = yielding(20.0, 0.15);
    let p = plan(&text, "N2");
    assert_eq!(p.hand_members, ["R1"], "{p:?}");
    assert!(p.hand[0].contains("too short"), "{:?}", p.hand);
    assert_eq!(p.kept_free, ["R1"]);
    assert!(p.ends.iter().any(|e| e.member == "D1"), "{p:?}");
    let w = written(&text, &p);
    let j = joint(&w, "N2");
    for c in &j.clashes {
        let m = [c.a.member(), c.b.member()];
        assert!(
            !(m.contains(&Some("D1")) && m.contains(&Some("R1"))),
            "{}",
            c.words()
        );
    }
    // Without its place kept (planned as before: D1 first, concentric) D1
    // would run into R1 as it is.
    let alone = K.replace(
        "[[member]]\nid = \"D2\"\nnodes = [\"N5\", \"N2\"]\nsection = \"EA-61x7\"\norientation = \"z-y-\"\ndesign = { class = \"bracing\" }\n",
        "",
    );
    let q = plan(&alone, "N2");
    let free = cut_of(&q, "D1").unwrap();
    assert!(cut_of(&p, "D1").unwrap() > free, "{p:?} {q:?}");
}

/// A main angle running on through a joint in the middle of a panel, two
/// diagonals of its face bolted directly on its flange with two bolts each,
/// their ends close together (direct flange connections, step 2a: an
/// example joint the owner sent, 2026-11-19, rebuilt here).
#[test]
fn diagonals_bolt_directly_on_a_main_angle_running_through() {
    // A main angle much wider than the diagonals (an invented section
    // added to the example library for this joint).
    let lib = crate::SectionLibrary::parse(&format!(
        "{}\n[[section]]\nid = \"EA-160x15\"\nname = \"Equal angle 160 × 15 (invented)\"\n\
         shape = \"equal_angle\"\nh = \"160 mm\"\nt = \"15 mm\"\nr = \"16 mm\"\n",
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../libraries/sections/example-sections.ohlsec"),
        )
        .unwrap()
    ))
    .unwrap();
    let text = format!(
        "{}\n[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"L2\"\n",
        K.replace(
            "design = { class = \"bracing\" }",
            "design = { class = \"bracing\", bolts = { diameter = \"12 mm\", count = 2 } }",
        )
        .replace(
            "section = \"EA-61x7\"\norientation = \"x+y+\"",
            "section = \"EA-160x15\"\norientation = \"x+y+\"",
        )
    );
    let at = |text: &str| {
        joint_details(text, &lib, &Default::default())
            .unwrap()
            .joint("N2")
            .unwrap()
            .clone()
    };
    let p = plan_joint(&at(&text), &settings(), &|_, _| None);
    assert!(p.hand.is_empty(), "{:?}", p.hand);
    assert!(p.plates.is_empty(), "{p:?}");
    let ends: Vec<&str> = p.ends.iter().map(|e| e.member.as_str()).collect();
    assert_eq!(ends, ["D1", "D2"]);
    for e in &p.ends {
        assert!(
            matches!(&e.onto, Onto::Member(m) if m.starts_with('L')),
            "{e:?}"
        );
        assert!(e.off.abs() < 1e-9, "concentric: {e:?}");
    }
    let j = at(&written(&text, &p));
    let laid = |x: &crate::ClashPart| x.member().is_some_and(|m| m.starts_with('D'));
    let mine: Vec<String> = j
        .clashes
        .iter()
        .filter(|c| laid(&c.a) || laid(&c.b))
        .map(|c| c.words())
        .collect();
    assert!(mine.is_empty(), "{mine:?}");
}

/// The same joint with the diagonals' bolts too many for the main angle's
/// flange: one gusset plate on the flange (as thick as the thickest leg on
/// it), both diagonals bolted onto it (a mid-panel gusset plate, step 2b:
/// the owner's third example, 2026-11-19, rebuilt here).
#[test]
fn a_gusset_plate_on_a_main_angle_in_the_middle_of_a_panel() {
    let text = format!(
        "{}\n[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"L2\"\n",
        K.replace(
            "design = { class = \"bracing\" }",
            "design = { class = \"bracing\", bolts = { diameter = \"16 mm\", count = 4 } }",
        )
    );
    let p = plan(&text, "N2");
    assert!(p.hand.is_empty(), "{:?}", p.hand);
    assert_eq!(p.plates.len(), 1, "{p:?}");
    assert!((p.plates[0].thickness - 0.007).abs() < 1e-9);
    assert!(p.ends.iter().all(|e| e.onto == Onto::Plate(0)), "{p:?}");
    let w = written(&text, &p);
    assert!(
        clashes(&w, "N2", &p).is_empty(),
        "{:?}",
        clashes(&w, "N2", &p)
    );
    // Each diagonal naming its own member of the run (D1 onto L1, D2 onto
    // L2): one face of one flange, so still one plate.
    let named = format!(
        "{text}\n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"L1\"\n\
         \n[[connection]]\nmember = \"D2\"\njoint = \"N2\"\nonto = \"L2\"\n"
    );
    let q = plan(&named, "N2");
    assert!(q.hand.is_empty(), "{:?}", q.hand);
    assert_eq!(q.plates.len(), 1, "{q:?}");
}

/// A horizontal in two pieces of one section in line through a mid-panel
/// joint, a redundant standing on it: each piece of the horizontal works
/// out onto the redundant (the only member not in line with it), and the
/// redundant onto the horizontal running through.
pub(crate) const SPLIT: &str = r#"id = "S"
name = "s"
kind = "lattice"

[[node]]
id = "A"
point = ["-2 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "M"
point = ["0 m", "0 m", "0 m"]

[[node]]
id = "B"
point = ["2 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "T"
point = ["0 m", "0 m", "1.5 m"]
support = "fixed"

[[member]]
id = "H1"
nodes = ["A", "M"]
section = "EA-61x7"
orientation = "z+y-"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "H2"
nodes = ["M", "B"]
section = "EA-61x7"
orientation = "z+y-"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "V"
nodes = ["M", "T"]
section = "EA-61x7"
orientation = "y-x+"
design = { class = "redundant" }
"#;

/// Two members bolting onto each other (the does-not-fit investigation,
/// 2026-11-19: on real towers the pieces of a horizontal were cut back
/// onto a plate on the redundant that was itself laid onto them, its bolts
/// beyond their steel): never both laid out. The horizontal running
/// through stays the host, its pieces' ends left as they are, said; the
/// redundant is laid onto it; no end is laid onto a member whose own end
/// the plan lays out.
#[test]
fn members_bolting_onto_each_other_are_never_both_laid_out() {
    let j = joint(SPLIT, "M");
    let p = plan_joint(&j, &settings(), &|_, _| None);
    eprintln!("{j:#?}\n{p:#?}");
    let ends: Vec<&str> = p.ends.iter().map(|e| e.member.as_str()).collect();
    assert_eq!(ends, ["V"], "{p:?}");
    for m in ["H1", "H2"] {
        assert!(
            p.left
                .iter()
                .any(|l| l.starts_with(&format!("{m}: it would bolt onto V"))),
            "{:?}",
            p.left
        );
    }
    for e in &p.ends {
        let host = match &e.onto {
            Onto::Member(m) => m.clone(),
            Onto::Plate(k) => p.plates[*k].on.clone(),
        };
        assert!(!ends.contains(&host.as_str()), "{e:?} onto an end laid out");
    }
    // The same plan again.
    assert_eq!(plan_joint(&j, &settings(), &|_, _| None), p);
}

/// L1 (b), slightly more plates (owner 2026-11-19): a member the run's
/// clash rounds put onto a plate takes its face onto one, though its
/// bolts land on the flange (D1 and D2 lie on one face of L: both onto the
/// one plate); without, both bolted directly as before.
#[test]
fn a_face_put_onto_a_plate_goes_onto_one_though_its_bolts_land() {
    use std::collections::{BTreeMap, BTreeSet};
    // The main angle running through, wide (as in the owner's example of
    // diagonals bolted directly on it, above): the bolts land.
    let (lib, text) = wide_main();
    let j = joint_details(&text, &lib, &Default::default())
        .unwrap()
        .joint("N2")
        .unwrap()
        .clone();
    let s = settings();
    let direct = plan_joint(&j, &s, &|_, _| None);
    assert!(
        direct.plates.is_empty()
            && direct
                .ends
                .iter()
                .all(|e| matches!(e.onto, Onto::Member(_))),
        "{direct:?}"
    );
    let ctx = crate::joint_layout::PlanCtx {
        plated: BTreeSet::from(["D1".to_owned()]),
        ..Default::default()
    };
    let p = crate::joint_layout::plan_joint_ctx(
        &j,
        &s,
        &|_, _| None,
        &BTreeMap::new(),
        &BTreeMap::new(),
        crate::joint_layout::NODE_LIMIT,
        &ctx,
    );
    assert_eq!(p.plates.len(), 1, "{p:?}");
    assert_eq!(p.ends.len(), 2, "{p:?}");
    assert!(p.ends.iter().all(|e| e.onto == Onto::Plate(0)), "{p:?}");
}

/// L2 (c), 2026-11-20: an end on a plate the run's clash rounds ask bolted
/// directly instead (its bolts landing on the flange) is bolted directly,
/// the others of its face staying on the plate; with clash-aware planning
/// off and on alike.
#[test]
fn an_end_asked_bolted_directly_leaves_its_plate() {
    use std::collections::{BTreeMap, BTreeSet};
    let (lib, text) = wide_main();
    let j = joint_details(&text, &lib, &Default::default())
        .unwrap()
        .joint("N2")
        .unwrap()
        .clone();
    for exact in [false, true] {
        let s = LayoutSettings {
            exact_plan: exact,
            ..settings()
        };
        let ctx = crate::joint_layout::PlanCtx {
            plated: BTreeSet::from(["D1".to_owned()]),
            direct: BTreeSet::from(["D2".to_owned()]),
            ..Default::default()
        };
        let p = crate::joint_layout::plan_joint_ctx(
            &j,
            &s,
            &|_, _| None,
            &BTreeMap::new(),
            &BTreeMap::new(),
            crate::joint_layout::NODE_LIMIT,
            &ctx,
        );
        let onto = |m: &str| {
            p.ends
                .iter()
                .find(|e| e.member == m)
                .map(|e| e.onto.clone())
        };
        assert!(matches!(onto("D2"), Some(Onto::Member(_))), "{p:?}");
        assert_eq!(onto("D1"), Some(Onto::Plate(0)), "{p:?}");
        assert_eq!(p.plates.len(), 1, "{p:?}");
        assert_eq!(p.plates[0].members, vec!["D1".to_owned()], "{p:?}");
    }
}

/// The K-brace on a wide main angle running through N2 (an invented
/// 160 mm angle added to the example sections; its star of two, built
/// up, too), the diagonals with two 12 mm bolts each.
pub(crate) fn wide_main() -> (crate::SectionLibrary, String) {
    let lib = crate::SectionLibrary::parse(&format!(
        "{}\n[[section]]\nid = \"EA-160x15\"\nname = \"Equal angle 160 × 15 (invented)\"\n\
         shape = \"equal_angle\"\nh = \"160 mm\"\nt = \"15 mm\"\nr = \"16 mm\"\n\
         \n[[section]]\nid = \"2S-160x15\"\nname = \"Two angles 160 × 15 in a star \
         (invented)\"\nshape = \"star_angle\"\nh = \"160 mm\"\nt = \"15 mm\"\nr = \"16 mm\"\n\
         gap = \"11 mm\"\n",
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../libraries/sections/example-sections.ohlsec"),
        )
        .unwrap()
    ))
    .unwrap();
    let text = format!(
        "{}\n[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"L2\"\n",
        K.replace(
            "design = { class = \"bracing\" }",
            "design = { class = \"bracing\", bolts = { diameter = \"12 mm\", count = 2 } }",
        )
        .replace(
            "section = \"EA-61x7\"\norientation = \"x+y+\"",
            "section = \"EA-160x15\"\norientation = \"x+y+\"",
        )
    );
    (lib, text)
}

/// L1 (d): the members an end could bolt onto, in today's rule's order —
/// those running through the joint first (the leg's piece on the end's
/// side, then the other), then the other diagonal ending there.
#[test]
fn the_members_an_end_could_bolt_onto_in_todays_order() {
    let j = joint(K, "N2");
    let d1 = j.arrivals.iter().find(|a| a.member == "D1").unwrap();
    assert_eq!(
        crate::joint_layout::onto_candidates(&j, d1),
        ["L1", "L2", "D2"]
    );
}

/// L5 (2026-11-20): an end of a face going onto a plate bolted directly
/// where pulled back further, or moved by at most the plate's thickness,
/// its bolts land — kept only where the joint then lays out more ends, or
/// as many on fewer plates (the guard). On the invented joints: never fewer
/// ends, never more plates, no new clash, the same plan twice, and a move
/// at most the plate's thickness.
#[test]
fn ends_moved_to_bolt_directly_never_lay_out_less() {
    let bolts = |c: u32| {
        K.replace(
            "design = { class = \"bracing\" }",
            &format!("design = {{ class = \"bracing\", bolts = {{ diameter = \"16 mm\", count = {c} }} }}"),
        )
    };
    let texts = [
        (K.to_owned(), "N2"),
        (bolts(1), "N2"),
        (bolts(4), "N2"),
        (bolts(6), "N2"),
        (SHALLOW.to_owned(), "N2"),
        (CORNER.to_owned(), "N2"),
        (yielding(10.0, 1.0), "N2"),
        (yielding(25.0, 0.6), "N2"),
        (SPLIT.to_owned(), "M"),
    ];
    for (text, id) in &texts {
        let Ok(d) = joint_details(text, &library(), &Default::default()) else {
            continue;
        };
        let Some(j) = d.joint(id) else { continue };
        let off = LayoutSettings {
            direct_moved: false,
            ..settings()
        };
        let a = plan_joint(j, &off, &|_, _| None);
        let b = plan_joint(j, &settings(), &|_, _| None);
        assert!(
            b.ends.len() > a.ends.len()
                || (b.ends.len() == a.ends.len() && b.plates.len() <= a.plates.len()),
            "{id}: {a:?} -> {b:?}"
        );
        assert_eq!(b, plan_joint(j, &settings(), &|_, _| None));
        if b != a {
            let w = written(text, &b);
            assert!(clashes(&w, id, &b).is_empty(), "{:?}", clashes(&w, id, &b));
        }
        for e in &b.ends {
            if let Some(sh) = e.shift
                && a.ends
                    .iter()
                    .any(|x| x.member == e.member && x.shift != e.shift)
            {
                assert!(sh.abs() <= 0.007 + 1e-9, "{e:?}");
            }
        }
    }
}

/// The K-brace with a wider leg running through the joint (an invented
/// 160 × 14 angle) and two bolts on each diagonal: concentric, both pulled
/// back clear of each other, their bolts past the leg's flange — a plate.
/// With L5 one diagonal lies at the work point and the other is pulled
/// back clear of it and moved a few millimetres: both bolted directly onto
/// the leg, no plate.
#[test]
fn a_k_brace_on_a_wide_leg_bolts_directly_with_a_small_move() {
    let base = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
    )
    .unwrap();
    let lib = crate::SectionLibrary::parse(&format!(
        "{base}\n[[section]]\nid = \"EA-160x14\"\nname = \"Equal angle 160 × 14 (invented)\"\n\
         shape = \"equal_angle\"\nh = \"160 mm\"\nt = \"14 mm\"\nr = \"12 mm\"\n"
    ))
    .unwrap();
    let text = format!(
        "{}\n[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"L2\"\n",
        K.replace(
            "section = \"EA-61x7\"\norientation = \"x+y+\"",
            "section = \"EA-160x14\"\norientation = \"x+y+\""
        )
        .replace(
            "design = { class = \"bracing\" }",
            "design = { class = \"bracing\", bolts = { diameter = \"16 mm\", count = 2 } }"
        )
        .replace(
            "[\"2.5 m\", \"0 m\", \"0 m\"]",
            "[\"3.6 m\", \"0 m\", \"0 m\"]"
        )
        .replace(
            "[\"2.5 m\", \"0 m\", \"6 m\"]",
            "[\"3.6 m\", \"0 m\", \"6 m\"]"
        )
    );
    let d = joint_details(&text, &lib, &Default::default()).unwrap();
    let j = d.joint("N2").unwrap();
    let off = LayoutSettings {
        direct_moved: false,
        ..settings()
    };
    let a = plan_joint(j, &off, &|_, _| None);
    assert_eq!(a.plates.len(), 1, "{a:?}");
    let b = plan_joint(j, &settings(), &|_, _| None);
    assert!(b.hand.is_empty(), "{:?}", b.hand);
    assert!(b.plates.is_empty(), "{b:?}");
    assert_eq!(b.ends.len(), 2);
    assert!(
        b.ends
            .iter()
            .all(|e| matches!(&e.onto, Onto::Member(m) if m == "L1" || m == "L2")),
        "{b:?}"
    );
    // A move of at most the plate's thickness (the diagonals' 7 mm legs),
    // said in the notes.
    assert!(
        b.ends
            .iter()
            .all(|e| e.shift.unwrap_or(0.0).abs() <= 0.007 + 1e-9),
        "{b:?}"
    );
    assert!(b.notes.iter().any(|n| n.contains("moved")), "{:?}", b.notes);
    assert_eq!(b, plan_joint(j, &settings(), &|_, _| None));
}
