// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The joints left for review (O44 L31): the record's reasons split and
//! grouped, the counts, the tick written and read, cleared by a change of
//! the joint's layout and only by that, the text and CSV.

use crate::joint_review::{
    NO_REASON, REVIEWED_KEY, reason_group, reasons_of, review_list, set_reviewed,
};
use crate::layout_run_tests::{run, settings};

/// An invented body: two joints where members meet (J1, J2), a leg tip
/// (T) with one member; the layout record says J1 laid out with a member
/// left, J2 not laid out with two members left, and nothing of a third
/// joint J3 where two members meet.
const BODY: &str = r#"format = "ohl-structure"
version = 28
id = "R"
name = "review"
kind = "lattice"

[[node]]
id = "J1"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "J2"
point = ["0 m", "0 m", "3 m"]

[[node]]
id = "J3"
point = ["2 m", "0 m", "3 m"]

[[node]]
id = "T"
point = ["0 m", "0 m", "6 m"]

[[member]]
id = "A1"
nodes = ["J1", "J2"]
section = "EA-61x7"
design = { class = "leg" }

[[member]]
id = "A2"
nodes = ["J2", "T"]
section = "EA-61x7"
design = { class = "leg" }

[[member]]
id = "B1"
nodes = ["J1", "J3"]
section = "EA-61x7"
design = { class = "bracing" }

[[member]]
id = "B2"
nodes = ["J3", "J2"]
section = "EA-61x7"
design = { class = "bracing" }

[[connection]]
member = "B1"
joint = "J1"
cut = "40 mm"

[[generator]]
id = "joint-layout"
kind = "joint_layout"
joints = ["J1"]
hand = [["J1", "B1: its axis runs 4° out of the face of A1's flange it bolts onto"], ["J2", "A2: laid out, A2 and B2 would run 4 mm into each other (left as it was); B2: its bolts join nothing — its 2 bolt(s) (its member's design data) are drawn at this joint, but it bolts onto nothing here; by hand: what it bolts onto or its splice given, or the pieces in line confirmed one part"]]
"#;

#[test]
fn a_joints_reasons_are_split_one_per_member() {
    let joined = "D4: its bolted leg lies 45° off the flange of L2 it bolts onto and no turn \
                  lays it flat at both its ends; a welded plate in its own plane does not fit: \
                  on the inner face of L2's flange it runs towards the angle's other leg (turn \
                  it, or lay it out by hand); D5: on a plate on L2, the plate would run into D6 \
                  (its outline round the work point; lay it out by hand); laid out: D6 and a \
                  plate on L2 would run 3 mm into each other";
    let r = reasons_of(joined);
    assert_eq!(r.len(), 3, "{r:#?}");
    assert!(
        r[0].starts_with("D4: ") && r[0].contains("a welded plate"),
        "{r:?}"
    );
    assert!(
        r[1].starts_with("D5: ") && r[1].ends_with("by hand)"),
        "{r:?}"
    );
    assert!(r[2].starts_with("laid out: D6"), "{r:?}");
    // "by hand:" begins no reason of its own.
    let r = reasons_of(
        "B2: its bolts join nothing — here; by hand: what it bolts onto or its splice given",
    );
    assert_eq!(r.len(), 1, "{r:?}");
    assert!(reasons_of("").is_empty());
}

#[test]
fn each_recorded_reason_falls_in_its_group() {
    let cases = [
        (
            "C1: its bolts do not all land on L1's flange through splice S1's outer cover, and \
             a plate over the cover is not laid out (lay it out by hand)",
            0,
        ),
        (
            "where the splice is is not known (its legs drawn lapped, as given)",
            0,
        ),
        (
            "B2: its bolts join nothing — its 1 bolt(s) are drawn at this joint, but it bolts \
             onto nothing here",
            1,
        ),
        (
            "D1: its axis runs 3° out of the face of L1's flange it bolts onto",
            2,
        ),
        (
            "D1: its bolted leg lies 45° off the flange of L1 it bolts onto and no turn lays it \
             flat at both its ends",
            3,
        ),
        (
            "D1: on the inner face of L1's flange it runs towards the angle's other leg",
            3,
        ),
        (
            "D1: its bolts do not all land on L1's flange, and L1 is built up: a plate on it is \
             drawn through the work point, not on its angle (lay it out by hand)",
            4,
        ),
        (
            "D1: across the gap between L1's angles, a plate between them would be 1.7 mm thick",
            4,
        ),
        (
            "D1: laid out, D2 and a plate on L1 would run 4 mm into each other (left as it was)",
            5,
        ),
        (
            "laid out: D2 and the head of a holding bolt of a plate on L1 would run 4 mm into \
             each other",
            5,
        ),
        (
            "D1: on a plate on L1, the plate would run into D3 (its outline round the work \
             point; lay it out by hand)",
            6,
        ),
        (
            "D1: 82 mm pulled back to clear D2 (the longest 26 mm; no shift up to 30 mm fits)",
            7,
        ),
        (
            "D1: on a plate its bolts need it pulled back 103 mm to pass the flange of L1 (the \
             longest pull-back 53 mm)",
            7,
        ),
        (
            "L2: laid out, F1 (as it is) would then not fit onto it: its bolt -14 mm from L2's \
             heel (left as it was)",
            8,
        ),
        (
            "D1: a bolt of a member left as it is that bolts onto it would not stay on it",
            8,
        ),
        (
            "D1 and D2 within 0°: no room on one plate (D1 needs 9999 mm pulled back)",
            9,
        ),
        ("D1: too short for its bolts (75 mm to half its length)", 10),
        ("D1: something the list does not know", 11),
    ];
    for (why, rank) in cases {
        assert_eq!(reason_group(why).rank, rank, "{why}");
    }
    // Each group's name names what it is.
    assert!(
        reason_group("D1: too short for its bolts")
            .name
            .contains("too short")
    );
}

#[test]
fn the_list_counts_the_joints_whole_and_left_and_groups_them() {
    let list = review_list(BODY).unwrap();
    assert!(list.run);
    // J1, J2, J3 (two or more members meet); the tip T not counted.
    assert_eq!(
        (list.total, list.whole, list.left()),
        (3, 0, 3),
        "{list:#?}"
    );
    let joints: Vec<&str> = list.rows.iter().map(|r| r.joint.as_str()).collect();
    assert_eq!(joints, ["J1", "J2", "J3"]);
    let j2 = list.row("J2").unwrap();
    assert_eq!(j2.reasons.len(), 2, "{:?}", j2.reasons);
    // Its first reason in the groups' order: bolts joining nothing (1)
    // before a clash after writing (5).
    assert_eq!(j2.group.rank, 1, "{:?}", j2.group);
    assert!(!j2.written);
    assert!(list.row("J1").unwrap().written);
    assert_eq!(list.row("J3").unwrap().group.name, NO_REASON);
    let groups = list.groups();
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].0.rank, 1);
    assert_eq!(groups.last().unwrap().0.name, NO_REASON);
    let by: Vec<&str> = list.by_group().iter().map(|r| r.joint.as_str()).collect();
    assert_eq!(by, ["J2", "J1", "J3"]);
    assert_eq!(
        list.summary(),
        "0 of 3 joints laid out whole, 3 left (0 reviewed)"
    );
    // J1 laid out whole once its reason is gone.
    let whole = BODY.replace(
        "[\"J1\", \"B1: its axis runs 4° out of the face of A1's flange it bolts onto\"], ",
        "",
    );
    let list = review_list(&whole).unwrap();
    assert_eq!((list.total, list.whole, list.left()), (3, 1, 2));
}

#[test]
fn a_tick_is_stored_with_the_joint_and_read_back_and_older_files_have_none() {
    let list = review_list(BODY).unwrap();
    assert_eq!(list.reviewed(), 0);
    let j2 = list.row("J2").unwrap().clone();
    let mut doc: toml_edit::DocumentMut = BODY.parse().unwrap();
    let n = set_reviewed(&mut doc, &[("J2".into(), Some(j2.stamp.clone()))]).unwrap();
    assert_eq!(n, 1);
    let text = doc.to_string();
    // The file raised to version 29, the tick on J2's [[node]].
    assert!(text.contains("version = 29"), "{text}");
    assert!(text.contains(&format!("{REVIEWED_KEY} = \"{}\"", j2.stamp)));
    let t = crate::layout_run_tests::parse(&text);
    assert_eq!(
        t.nodes.iter().find(|n| n.id == "J2").unwrap().reviewed,
        Some(j2.stamp.clone())
    );
    let list = review_list(&text).unwrap();
    assert!(list.row("J2").unwrap().reviewed);
    assert_eq!(list.reviewed(), 1);
    assert!(list.summary().ends_with("(1 reviewed)"));
    // The same again changes nothing; taken off.
    assert_eq!(
        set_reviewed(&mut doc, &[("J2".into(), Some(j2.stamp.clone()))]).unwrap(),
        0
    );
    assert_eq!(set_reviewed(&mut doc, &[("J2".into(), None)]).unwrap(), 1);
    assert!(!doc.to_string().contains(REVIEWED_KEY));
    assert!(
        !review_list(&doc.to_string())
            .unwrap()
            .row("J2")
            .unwrap()
            .reviewed
    );
    // A joint not in the file: refused, nothing written.
    let before = doc.to_string();
    let e = set_reviewed(
        &mut doc,
        &[("J2".into(), Some("x".into())), ("NONE".into(), None)],
    )
    .unwrap_err();
    assert!(e.contains("NONE"), "{e}");
    assert_eq!(doc.to_string(), before);
}

#[test]
fn a_change_of_the_joints_layout_clears_its_tick_and_nothing_else_does() {
    let list = review_list(BODY).unwrap();
    let mut doc: toml_edit::DocumentMut = BODY.parse().unwrap();
    let ticks: Vec<(String, Option<String>)> = list
        .rows
        .iter()
        .map(|r| (r.joint.clone(), Some(r.stamp.clone())))
        .collect();
    set_reviewed(&mut doc, &ticks).unwrap();
    let ticked = doc.to_string();
    assert_eq!(review_list(&ticked).unwrap().reviewed(), 3);
    // A value at J1 changed by hand: J1's tick cleared, the others kept.
    let changed = ticked.replace("cut = \"40 mm\"", "cut = \"60 mm\"");
    assert_ne!(changed, ticked);
    let list = review_list(&changed).unwrap();
    let j1 = list.row("J1").unwrap();
    assert!(!j1.reviewed && j1.cleared, "{j1:?}");
    assert!(list.row("J2").unwrap().reviewed && list.row("J3").unwrap().reviewed);
    // J2's reasons changed (a run laying it out otherwise): cleared.
    let changed = ticked.replace("would run 4 mm", "would run 5 mm");
    let list = review_list(&changed).unwrap();
    assert!(list.row("J2").unwrap().cleared);
    assert!(list.row("J1").unwrap().reviewed);
    // A member meeting at J3 turned: J3 and the other joint of that member
    // (J1) cleared, J2 kept.
    let changed = ticked.replace(
        "nodes = [\"J1\", \"J3\"]\nsection = \"EA-61x7\"",
        "nodes = [\"J1\", \"J3\"]\nsection = \"EA-61x7\"\nroll = \"90 deg\"",
    );
    let list = review_list(&changed).unwrap();
    assert!(list.row("J3").unwrap().cleared && list.row("J1").unwrap().cleared);
    assert!(list.row("J2").unwrap().reviewed);
    // The file written otherwise, its values the same (spacing, the order
    // of a table's keys, a comment): every tick kept.
    let changed = ticked
        .replace("cut = \"40 mm\"", "cut   =   \"40 mm\"  # by hand")
        .replace(
            "id = \"A1\"\nnodes = [\"J1\", \"J2\"]",
            "nodes = [\"J1\", \"J2\"]\nid = \"A1\"",
        );
    assert_ne!(changed, ticked);
    assert_eq!(review_list(&changed).unwrap().reviewed(), 3);
    // A value the layout does not take (a member's buckling factor):
    // kept.
    let changed = ticked.replace(
        "id = \"B2\"\nnodes = [\"J3\", \"J2\"]",
        "id = \"B2\"\nnodes = [\"J3\", \"J2\"]\nk = 0.9",
    );
    assert_eq!(review_list(&changed).unwrap().reviewed(), 3);
}

#[test]
fn a_run_laying_a_joint_out_otherwise_clears_its_tick() {
    // The K-brace laid out, its joint's tick, then the joint's layout
    // changed by a run with another gap.
    let text = crate::joint_layout_tests::K.to_owned();
    let r = run(&text, &settings(), None, false);
    let list = review_list(&r.text).unwrap();
    // N2 laid out whole: nothing left to review.
    assert_eq!(
        (list.total, list.whole, list.left()),
        (1, 1, 0),
        "{list:#?}"
    );
    // Its pull-back made too short to fit (as an edit that no longer
    // fits): N2 left, said with why.
    let edited = r
        .text
        .replace("pull_back = \"300 mm\"", "pull_back = \"1 mm\"");
    let f = crate::layout_run::follow(
        &r.text,
        &edited,
        &crate::joints_tests::library(),
        &Default::default(),
        &[],
    )
    .expect("followed");
    let list = review_list(&f.text).unwrap();
    assert_eq!(list.left(), 1, "{list:#?}");
    let n2 = list.row("N2").unwrap().clone();
    assert!(!n2.reasons.is_empty() && n2.written);
    let mut doc: toml_edit::DocumentMut = f.text.parse().unwrap();
    set_reviewed(&mut doc, &[("N2".into(), Some(n2.stamp.clone()))]).unwrap();
    let ticked = doc.to_string();
    assert!(review_list(&ticked).unwrap().row("N2").unwrap().reviewed);
    // Laid out again with the longest pull-back back: its layout changed,
    // its tick cleared (whole and off the list, or left and not reviewed).
    let again = run(
        &ticked.replace("pull_back = \"1 mm\"", "pull_back = \"300 mm\""),
        &settings(),
        None,
        false,
    );
    let list = review_list(&again.text).unwrap();
    assert!(
        list.row("N2").is_none_or(|r| !r.reviewed && r.cleared),
        "{list:#?}"
    );
    // A file without a run: every joint where members meet is left.
    let list = review_list(&text).unwrap();
    assert!(!list.run);
    assert_eq!((list.total, list.whole, list.left()), (1, 0, 1));
    assert!(list.text().starts_with("No joint layout run recorded"));
}

#[test]
fn the_list_as_text_and_csv() {
    let list = review_list(BODY).unwrap();
    let csv = list.csv();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "joint,group,reviewed,laid_out_in_part,reasons");
    assert_eq!(lines.len(), 4);
    assert!(
        lines[1].starts_with("J2,its bolts join nothing,no,no,\""),
        "{}",
        lines[1]
    );
    assert!(lines[2].starts_with("J1,an end out of the flange's plane,no,yes,"));
    assert!(
        lines[3].starts_with("J3,\"no reason recorded"),
        "{}",
        lines[3]
    );
    let text = list.text();
    assert!(text.contains("0 of 3 joints laid out whole, 3 left (0 reviewed)"));
    assert!(text.contains("By reason:"));
    assert!(text.contains("J2\tits bolts join nothing\t-"));
    assert!(text.contains("the reviewer's mark only"));
}
