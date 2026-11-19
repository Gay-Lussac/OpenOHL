// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Towers shared by several routes ([`crate::shared`], phase 8): one test
//! per rule refused, the changes only moving the other route allowed with
//! its stale mark, a protected line, parts, merges, and random commands
//! with a working set (an accepted one never leaves the other route's
//! objects changed or not valid; the same command is accepted without a
//! working set).
//!
//! The line: the example's two routes, `main` and `branch`, sharing the
//! Y pylon T3 (alignment point P2 tied to it on both alignments). Section
//! S3 of `branch` ends at T3's set 2; the jumper J-T3-branch joins it to
//! S1 of `main` at T3 (on both routes) and is given a support clamp on
//! T3's wire point JW (string SJ from A2). T3 and T6 carry a spare set 7
//! (strings S7), and a structure group names T1 (main) and T6 (branch).
//! Ana (admin) works on `main` only.

use super::*;
use crate::in_place_tests::Rng;
use crate::shared::{STALE_KEY, WORKING_SETS_KEY};
use crate::versions::{Choices, Side};
use ohl_line::SectionPoint;
use std::path::PathBuf;

fn line() -> Line {
    let mut line = Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap();
    let t3 = line.structures.iter_mut().find(|s| s.id == "T3").unwrap();
    let a2 = t3.points.iter().find(|p| p.label == "A2").unwrap().position;
    t3.wires.push(ohl_line::WirePoint {
        label: "JW".into(),
        position: [a2[0], a2[1], a2[2] - 1.5],
        ..Default::default()
    });
    t3.strings.push(ohl_line::InsulatorString {
        label: "SJ".into(),
        top: "A2".into(),
        wire: "JW".into(),
        length: 1.5,
        ..Default::default()
    });
    for id in ["T3", "T6"] {
        let s = line.structures.iter_mut().find(|s| s.id == id).unwrap();
        let a = s.points[0].clone();
        s.wires.push(ohl_line::WirePoint {
            label: "W7".into(),
            position: [a.position[0], a.position[1], a.position[2] - 3.5],
            set: Some(7),
            ..Default::default()
        });
        s.strings.push(ohl_line::InsulatorString {
            label: "S7".into(),
            top: a.label.clone(),
            wire: "W7".into(),
            length: 3.5,
            ..Default::default()
        });
    }
    let t3 = line.structure("T3").unwrap();
    let (w1, w2) = (
        t3.wire_at_set(1).unwrap().position,
        t3.wire_at_set(2).unwrap().position,
    );
    let chord =
        ((w1[0] - w2[0]).powi(2) + (w1[1] - w2[1]).powi(2) + (w1[2] - w2[2]).powi(2)).sqrt();
    let j = line
        .jumpers
        .iter_mut()
        .find(|j| j.id == "J-T3-branch")
        .unwrap();
    j.sag = None;
    j.length = Some(chord + 2.0);
    j.set_supports(vec![ohl_line::JumperClamp {
        label: String::new(),
        structure: None,
        wire: "JW".into(),
        at: 0.5,
        clamp_length: 0.1,
    }]);
    line.structure_groups.push(ohl_line::StructureGroup {
        name: "both routes".into(),
        structures: vec!["T1".into(), "T6".into()],
        ..Default::default()
    });
    line.validate().unwrap();
    line
}

fn open(name: &str) -> (PathBuf, Session) {
    let dir = std::env::temp_dir().join(format!("ohl-shared-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    for ext in ["", "-wal", "-shm"] {
        std::fs::remove_file(format!("{}{ext}", path.display())).ok();
    }
    let mut s = Session::open(&path).unwrap();
    s.import(&line(), "ana").unwrap();
    s.set_user("ana", "ben", Role::Designer).unwrap();
    (path, s)
}

/// A session where ana works on `main` only.
fn on_main(name: &str) -> (PathBuf, Session) {
    let (path, mut s) = open(name);
    s.set_working_set("ana", Some(["main".to_owned()].into()))
        .unwrap();
    (path, s)
}

fn structure(s: &Session, id: &str) -> Structure {
    s.line().structure(id).unwrap().clone()
}

fn put_structure(t: &Structure) -> Command {
    Command::Put {
        kind: Kind::Structure,
        id: t.id.clone(),
        value: serde_json::to_value(t).unwrap(),
        ord: None,
    }
}

/// `command` refused for ana with a message holding every one of `words`.
fn refused_with(s: &mut Session, command: Command, words: &[&str]) {
    let before = s.line().clone();
    let e = s
        .apply("ana", command.clone())
        .expect_err(&format!("{command:?}"));
    let text = e.to_string();
    for w in words {
        assert!(text.contains(w), "{w:?} not in: {text}");
    }
    assert_eq!(*s.line(), before, "nothing changed");
}

fn moved(t: &Structure, dx: f64) -> Command {
    Command::MoveStructure {
        id: t.id.clone(),
        position: [t.position[0] + dx, t.position[1], t.position[2]],
    }
}

#[test]
fn rule_0_the_other_routes_objects_are_not_changed() {
    let (_, mut s) = on_main("rule0");
    let s3 = s
        .line()
        .sections
        .iter()
        .find(|x| x.id == "S3")
        .unwrap()
        .clone();
    refused_with(
        &mut s,
        Command::SetStringing {
            section: "S3".into(),
            stringing: s3.stringing.clone(),
        },
        &["section S3 is of route branch", "not in your working set"],
    );
    // A new section on the branch's towers only.
    let mut copy = s3.clone();
    copy.id = "S3b".into();
    copy.points = vec![SectionPoint::new("T6", 7), SectionPoint::new("T3", 7)];
    copy.span_points.clear();
    refused_with(
        &mut s,
        put(Kind::Section, "S3b", &copy),
        &["S3b", "route branch", "cannot be made here"],
    );
    // Its alignment and its tower T6 (on it only).
    refused_with(
        &mut s,
        Command::InsertAlignmentPoint {
            route: "branch".into(),
            index: 2,
            position: [640.0, 400.0, 500.0],
            point: None,
        },
        &["route branch", "not in your working set"],
    );
    let t6 = structure(&s, "T6");
    refused_with(
        &mut s,
        moved(&t6, 1.0),
        &["structure T6 is route branch's only"],
    );
    // Ben, with no working set, changes them.
    s.apply(
        "ben",
        Command::SetStringing {
            section: "S3".into(),
            stringing: s3.stringing,
        },
    )
    .unwrap();
    s.apply("ben", moved(&t6, 1.0)).unwrap();
}

#[test]
fn rule_1_a_used_structure_is_not_deleted_or_renamed() {
    let (_, mut s) = on_main("rule1");
    refused_with(
        &mut s,
        Command::DeleteStructure {
            id: "T3".into(),
            confirm: true,
        },
        &["structure T3 is used by", "route branch", "rule 1"],
    );
    refused_with(
        &mut s,
        Command::Rename {
            kind: Kind::Structure,
            id: "T3".into(),
            new_id: "T3x".into(),
        },
        &["structure T3 is used by", "rule 1"],
    );
}

#[test]
fn rule_2_a_used_wire_point_stays() {
    let (_, mut s) = on_main("rule2");
    refused_with(
        &mut s,
        Command::DeleteAttachment {
            structure: "T3".into(),
            set: 2,
            phase: None,
            confirm: true,
        },
        &[
            "wire point set 2 of T3 is used by section S3 of route branch",
            "rule 2",
        ],
    );
    refused_with(
        &mut s,
        Command::RenameAttachment {
            structure: "T3".into(),
            set: 2,
            phase: None,
            new_set: 5,
            new_phase: None,
        },
        &["set 2 of T3", "rule 2"],
    );
    let mut t3 = structure(&s, "T3");
    t3.omit_sets.push(2);
    refused_with(&mut s, put_structure(&t3), &["left out", "rule 2"]);
}

#[test]
fn rule_3_what_it_hangs_on_stays() {
    let (_, mut s) = on_main("rule3");
    let mut t3 = structure(&s, "T3");
    // String S2 (to W2, set 2) hung from A1 instead of A2.
    t3.strings.iter_mut().find(|x| x.label == "S2").unwrap().top = "A1".into();
    refused_with(
        &mut s,
        put_structure(&t3),
        &[
            "set 2 of T3 is used by section S3",
            "what it hangs on",
            "rule 3",
        ],
    );
    // Its string removed: fixed instead of hanging (rule 4 words).
    let mut t3 = structure(&s, "T3");
    t3.strings.retain(|x| x.label != "S2");
    refused_with(
        &mut s,
        put_structure(&t3),
        &["set 2 of T3", "fixed to the structure"],
    );
}

#[test]
fn rule_4_suspension_stays_suspension() {
    let (_, mut s) = on_main("rule4");
    let mut t3 = structure(&s, "T3");
    t3.strings
        .iter_mut()
        .find(|x| x.label == "S2")
        .unwrap()
        .strain = true;
    refused_with(
        &mut s,
        put_structure(&t3),
        &["from a suspension point to a strain point", "rule 4"],
    );
}

#[test]
fn rule_5_a_support_clamp_keeps_its_strings() {
    let (_, mut s) = on_main("rule5");
    let mut t3 = structure(&s, "T3");
    t3.strings.iter_mut().find(|x| x.label == "SJ").unwrap().top = "A1".into();
    refused_with(
        &mut s,
        put_structure(&t3),
        &[
            "JW of T3 holds a support clamp of jumper J-T3-branch",
            "rule 5",
        ],
    );
}

#[test]
fn rule_6_the_other_route_stays_on_it() {
    let (_, mut s) = on_main("rule6");
    let mut t3 = structure(&s, "T3");
    t3.routes = vec!["main".into()];
    refused_with(
        &mut s,
        put_structure(&t3),
        &["T3 is on route branch", "taken off it", "rule 6"],
    );
    refused_with(
        &mut s,
        Command::DetachAlignmentPoint {
            route: "main".into(),
            index: 1,
        },
        &[
            "alignment point P2 is on route branch",
            "untied from T3",
            "rule 6",
        ],
    );
}

#[test]
fn rule_7_a_group_naming_the_other_routes_structures() {
    let (_, mut s) = on_main("rule7");
    refused_with(
        &mut s,
        Command::Rename {
            kind: Kind::Structure,
            id: "T1".into(),
            new_id: "T1x".into(),
        },
        &[
            "structure group \"both routes\"",
            "names T6 of route branch",
            "rule 7",
        ],
    );
}

#[test]
fn rule_8_the_other_route_stays_valid() {
    let (_, mut s) = on_main("rule8");
    // T3 moved by a put, its tied point P2 (of both routes) left behind:
    // not valid, the other route named.
    let mut t3 = structure(&s, "T3");
    t3 = t3.moved_to([t3.position[0] + 1.0, t3.position[1], t3.position[2]]);
    refused_with(
        &mut s,
        put_structure(&t3),
        &["used by route branch, not in your working set"],
    );
}

#[test]
fn moving_a_shared_tower_marks_the_other_route_stale() {
    let (_, mut s) = on_main("stale");
    let t3 = structure(&s, "T3");
    let a = s.apply("ana", moved(&t3, 0.8)).unwrap();
    assert_eq!(
        a.notice.as_deref(),
        Some("shared towers: route branch's results are out of date (T3 moved 0.80 m)")
    );
    // Its tied point followed; the log entry says it.
    assert_eq!(structure(&s, "T3").position[0], t3.position[0] + 0.8);
    let entry = s.log().unwrap().pop().unwrap();
    assert!(
        entry
            .note
            .as_deref()
            .unwrap()
            .contains("route branch's results"),
        "{entry:?}"
    );
    let stale = s.stale_routes().unwrap();
    assert_eq!(stale.keys().collect::<Vec<_>>(), ["branch"]);
    assert_eq!(stale["branch"].notices[0].what, "T3 moved 0.80 m");
    assert_eq!(stale["branch"].notices[0].user, "ana");
    // Other changes only moving it: height, a string's insulator, a string
    // shared with main (S1 to W1, set 1: S1 of main and the jumper).
    let mut t3 = structure(&s, "T3");
    t3.height_adjust += 1.0;
    t3.strings
        .iter_mut()
        .find(|x| x.label == "S1")
        .unwrap()
        .insulator = Some("other".into());
    let p = t3.points.iter_mut().find(|p| p.label == "A1").unwrap();
    p.position[2] += 0.5;
    s.apply("ana", put_structure(&t3)).unwrap();
    let stale = s.stale_routes().unwrap();
    assert_eq!(stale["branch"].count, 2);
    assert!(
        stale["branch"].notices[1]
            .what
            .contains("height changed +1.00 m"),
        "{:?}",
        stale["branch"].notices[1]
    );
    // Undone: allowed too (it moves the branch again), stale as before.
    s.undo("ana").unwrap();
    s.undo("ana").unwrap();
    assert_eq!(s.stale_routes().unwrap()["branch"].count, 4);
    // Main's own towers: no mark.
    let t2 = structure(&s, "T2");
    assert!(s.apply("ana", moved(&t2, 1.0)).unwrap().notice.is_none());
    // Marked up to date by its users, not by ana.
    assert!(s.clear_stale("ana", "branch").is_err());
    s.clear_stale("ben", "branch").unwrap();
    assert!(s.stale_routes().unwrap().is_empty());
    // A setting: every outside route.
    let mut d = s.line().defaults;
    d.spacer_compression += 1.0;
    let c = s.defaults_command(d);
    let a = s.apply("ana", c).unwrap();
    assert!(a.notice.unwrap().contains("route branch's results"));
}

#[test]
fn hardware_only_the_other_route_uses_is_its_own() {
    let (_, mut s) = on_main("theirs");
    // Ben strings S9 of the branch on T3's spare set 7 (string S7 from A1).
    let mut s9 = s
        .line()
        .sections
        .iter()
        .find(|x| x.id == "S3")
        .unwrap()
        .clone();
    s9.id = "S9".into();
    s9.points = vec![SectionPoint::new("T3", 7), SectionPoint::new("T6", 7)];
    s9.span_points.clear();
    s.apply("ben", put(Kind::Section, "S9", &s9)).unwrap();
    // S7 holds W7 alone: the branch's hardware.
    let mut t3 = structure(&s, "T3");
    t3.strings
        .iter_mut()
        .find(|x| x.label == "S7")
        .unwrap()
        .length += 0.2;
    refused_with(
        &mut s,
        put_structure(&t3),
        &[
            "S7 of T3 is the hardware of section S9 of route branch only",
            "§7.4",
        ],
    );
    // A1 holds W1 too (S1 of main, the jumper of both): moved, the branch
    // marked stale.
    let mut a1 = structure(&s, "T3");
    a1.points
        .iter_mut()
        .find(|p| p.label == "A1")
        .unwrap()
        .position[2] += 0.3;
    let a = s.apply("ana", put_structure(&a1)).unwrap();
    assert!(a.notice.unwrap().contains("route branch's results"));
    // Ben, without a working set, may change S7.
    let mut t3 = structure(&s, "T3");
    t3.strings
        .iter_mut()
        .find(|x| x.label == "S7")
        .unwrap()
        .length += 0.2;
    s.apply("ben", put_structure(&t3)).unwrap();
}

#[test]
fn a_protected_line_needs_a_review() {
    let (_, mut s) = on_main("protected");
    s.commit("ana", "first").unwrap();
    s.protect_branch("ana", "main", true).unwrap();
    let t3 = structure(&s, "T3");
    refused_with(
        &mut s,
        moved(&t3, 0.5),
        &[
            "route branch",
            "is protected",
            "needs a review by its owners",
        ],
    );
    // Main's own tower: as before (ana is an admin).
    let t2 = structure(&s, "T2");
    s.apply("ana", moved(&t2, 0.5)).unwrap();
}

/// Without a working set (and with one holding every route) every
/// command answers as before, and nothing more is written.
#[test]
fn without_a_working_set_nothing_changes() {
    let command = |s: &Session, k: usize| {
        let t3 = structure(s, "T3");
        let mut strain = t3.clone();
        strain
            .strings
            .iter_mut()
            .find(|x| x.label == "S2")
            .unwrap()
            .strain = true;
        let mut a2 = t3.clone();
        a2.points
            .iter_mut()
            .find(|p| p.label == "A2")
            .unwrap()
            .position[2] += 0.3;
        match k {
            0 => moved(&t3, 0.8),
            1 => put_structure(&strain),
            2 => put_structure(&a2),
            3 => Command::DeleteAttachment {
                structure: "T3".into(),
                set: 7,
                phase: None,
                confirm: true,
            },
            4 => Command::Rename {
                kind: Kind::Structure,
                id: "T1".into(),
                new_id: "T1x".into(),
            },
            _ => Command::DeleteStructure {
                id: "T3".into(),
                confirm: true,
            },
        }
    };
    let (_, mut plain) = open("plain");
    let (_, mut all) = open("all");
    all.set_working_set("ana", Some(["main".to_owned(), "branch".to_owned()].into()))
        .unwrap();
    for k in 0..6 {
        let c = command(&plain, k);
        let (a, b) = (plain.apply("ana", c.clone()), all.apply("ana", c));
        assert_eq!(
            a.as_ref().map(|x| (&x.objects, &x.sections, &x.notice)),
            b.as_ref().map(|x| (&x.objects, &x.sections, &x.notice))
        );
        assert!(a.unwrap().notice.is_none());
    }
    assert_eq!(plain.line(), all.line());
    for key in [WORKING_SETS_KEY, STALE_KEY, crate::shared::PART_USERS_KEY] {
        assert_eq!(plain.conn.meta(key).unwrap(), None, "{key}");
    }
    assert!(!plain.shared_rules_in_use());
    // A working set taken back leaves no key.
    all.set_working_set("ana", None).unwrap();
    assert_eq!(all.conn.meta(WORKING_SETS_KEY).unwrap(), None);
    assert!(
        all.set_working_set("ana", Some(["nope".to_owned()].into()))
            .is_err()
    );
}

#[test]
fn a_part_checks_the_users_it_recorded() {
    let (dir, whole) = open("part-whole");
    let path = dir.parent().unwrap().join("part.ohl");
    std::fs::remove_file(&path).ok();
    let wanted: BTreeSet<(Kind, String)> = [
        (Kind::Structure, "T3".to_owned()),
        (Kind::Section, "S1".to_owned()),
    ]
    .into();
    whole.write_part(&wanted, &path, "ana").unwrap();
    let mut part = Session::open(&path).unwrap();
    // S3 and the jumper are not in it: recorded.
    let recorded = &part.part_users["T3"];
    let ids: BTreeSet<&str> = recorded.iter().map(|u| u.id.as_str()).collect();
    assert!(ids.contains("S3") && ids.contains("J-T3-branch"), "{ids:?}");
    let mut t3 = structure(&part, "T3");
    t3.strings
        .iter_mut()
        .find(|x| x.label == "S2")
        .unwrap()
        .strain = true;
    refused_with(
        &mut part,
        put_structure(&t3),
        &[
            "section S3 of route branch",
            "which this part does not hold",
            "rule 4",
        ],
    );
    let t3 = structure(&part, "T3");
    let a = part.apply("ana", moved(&t3, 0.5)).unwrap();
    assert!(a.notice.unwrap().contains("route branch's results"));
    std::fs::remove_file(&path).ok();
}

/// A merge where one side changes a structure and the other puts
/// something on it that the change breaks: a conflict.
#[test]
fn a_merge_breaking_a_connection_is_a_conflict() {
    let s9 = |s: &Session| {
        let mut x = s
            .line()
            .sections
            .iter()
            .find(|x| x.id == "S3")
            .unwrap()
            .clone();
        x.id = "S9".into();
        x.points = vec![SectionPoint::new("T3", 7), SectionPoint::new("T6", 7)];
        x.span_points.clear();
        x
    };
    // `main` drops T3's set 7 (or turns it to strain); `alt` strings S9 on it.
    let run = |name: &str, drop: bool, working_set: bool| {
        let (_, mut s) = open(name);
        if working_set {
            s.set_working_set("ben", Some(["branch".to_owned()].into()))
                .unwrap();
        }
        s.commit("ana", "first").unwrap();
        s.create_branch("ana", "alt", None).unwrap();
        let mut t3 = structure(&s, "T3");
        if drop {
            s.apply(
                "ana",
                Command::DeleteAttachment {
                    structure: "T3".into(),
                    set: 7,
                    phase: None,
                    confirm: true,
                },
            )
            .unwrap();
        } else {
            t3.strings
                .iter_mut()
                .find(|x| x.label == "S7")
                .unwrap()
                .strain = true;
            s.apply("ana", put_structure(&t3)).unwrap();
        }
        s.commit("ana", "T3 changed").unwrap();
        s.checkout("ana", "alt").unwrap();
        let x = s9(&s);
        s.apply("ana", put(Kind::Section, "S9", &x)).unwrap();
        s.commit("ana", "S9 strung").unwrap();
        s.checkout("ana", "main").unwrap();
        s
    };
    // Set 7 dropped: the merged line not valid — before, refused with
    // nothing to choose; now a conflict.
    let mut s = run("merge-drop", true, false);
    let plan = s.merge_plan("alt", &Choices::new()).unwrap();
    let c = plan
        .conflicts
        .iter()
        .find(|c| c.key == "structure:T3")
        .expect("a conflict");
    assert!(
        c.what.contains("section S9") && c.what.contains("rule 2"),
        "{}",
        c.what
    );
    let e = s.merge("ana", "alt", None, &Choices::new()).unwrap_err();
    assert!(e.to_string().contains("structure:T3"), "{e}");
    // The change kept (ours): S9 not taken.
    let ours: Choices = [("structure:T3".to_owned(), Side::Ours)].into();
    s.merge("ana", "alt", None, &ours).unwrap();
    assert!(s.line().sections.iter().all(|x| x.id != "S9"));
    let mut s = run("merge-drop-theirs", true, false);
    let theirs: Choices = [("structure:T3".to_owned(), Side::Theirs)].into();
    s.merge("ana", "alt", None, &theirs).unwrap();
    assert!(s.line().sections.iter().any(|x| x.id == "S9"));
    assert!(s.line().structure("T3").unwrap().wire_at_set(7).is_some());
    // Turned to strain: a valid line — as before, no conflict without
    // working sets…
    let s = run("merge-strain", false, false);
    let plan = s.merge_plan("alt", &Choices::new()).unwrap();
    assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
    // …a conflict with them.
    let s = run("merge-strain-ws", false, true);
    let plan = s.merge_plan("alt", &Choices::new()).unwrap();
    let c = plan
        .conflicts
        .iter()
        .find(|c| c.key == "structure:T3")
        .unwrap();
    assert!(c.what.contains("rule 4"), "{}", c.what);
}

/// The objects only `branch` has in the line (and the jumper between
/// both routes): their JSON, an alignment point's tie and words (its place
/// may follow a shared tower).
fn of_branch(line: &Line) -> BTreeMap<(Kind, String), Value> {
    let mut out = BTreeMap::new();
    for s in line.sections.iter().filter(|x| x.id == "S3") {
        out.insert((Kind::Section, s.id.clone()), json(s));
    }
    for j in line.jumpers.iter().filter(|x| x.id == "J-T3-branch") {
        out.insert((Kind::Jumper, j.id.clone()), json(j));
    }
    for s in line
        .structures
        .iter()
        .filter(|x| x.id == "T6" || x.id == "T7")
    {
        out.insert((Kind::Structure, s.id.clone()), json(s));
    }
    for a in line.alignments.iter().filter(|x| x.id == "branch") {
        out.insert((Kind::Alignment, a.id.clone()), json(a));
    }
    for r in line.routes.iter().filter(|x| x.id == "branch") {
        out.insert((Kind::Route, r.id.clone()), json(r));
    }
    for p in &line.alignment_points {
        if ["P2", "P4", "P5"].contains(&p.id.as_str()) {
            out.insert(
                (Kind::AlignmentPoint, p.id.clone()),
                serde_json::json!([p.structure, p.description]),
            );
        }
    }
    out
}

/// Random commands by ana (working on `main`) and the same on a project
/// without a working set: every command accepted with the working set is
/// accepted without it, with the same result; after it, `branch`'s
/// objects are as they were (their places aside where a shared tower took
/// them along) and the line is valid — its local validation of them
/// included.
#[test]
fn random_commands_never_break_the_other_route() {
    let mut accepted = 0;
    let mut refused_shared = 0;
    let seeds: u64 = std::env::var("OHL_SHARED_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12);
    for seed in 1..=seeds {
        let (_, mut ws) = on_main(&format!("random-ws-{seed}"));
        let (_, mut plain) = open(&format!("random-plain-{seed}"));
        let mut rng = Rng(seed * 7919 + 13);
        for step in 0..60 {
            let mut at = format!("seed {seed} step {step}");
            let before = of_branch(ws.line());
            let r = match rng.below(12) {
                0 => ws.undo("ana").map(|_| "undo"),
                1 => ws.redo("ana").map(|_| "redo"),
                k => {
                    let c = if k < 5 {
                        hardware_change(&ws, &mut rng)
                    } else {
                        crate::in_place_tests::command(&ws, &mut rng)
                    };
                    at = format!("{at} ({c:?})");
                    match ws.apply("ana", c.clone()) {
                        Ok(a) => {
                            let p = plain.apply("ana", c.clone());
                            let p = p.unwrap_or_else(|e| {
                                panic!("{at}: accepted with a working set, not without: {e} {c:?}")
                            });
                            assert_eq!(
                                (&a.objects, &a.sections),
                                (&p.objects, &p.sections),
                                "{at}"
                            );
                            Ok("do")
                        }
                        Err(e) => Err(e),
                    }
                }
            };
            match r {
                Ok(what) => {
                    accepted += 1;
                    if what != "do" {
                        // Undo and redo on the plain one too.
                        let p = if what == "undo" {
                            plain.undo("ana")
                        } else {
                            plain.redo("ana")
                        };
                        p.unwrap_or_else(|e| panic!("{at}: {what} without a working set: {e}"));
                    }
                    let after = of_branch(ws.line());
                    for (k, v) in &before {
                        assert_eq!(after.get(k), Some(v), "{at}: {k:?} of branch changed");
                    }
                    ws.line().validate().unwrap_or_else(|e| panic!("{at}: {e}"));
                    assert_eq!(ws.line(), plain.line(), "{at}");
                }
                Err(e) => {
                    let t = e.to_string();
                    if t.contains("working set") || t.contains("rule ") || t.contains("§7.4") {
                        refused_shared += 1;
                    }
                }
            }
        }
    }
    eprintln!("{accepted} accepted, {refused_shared} refused by the rules");
    assert!(
        accepted > 100 && refused_shared > 20,
        "{accepted} accepted, {refused_shared} refused by the rules"
    );
}

/// A random change of a structure's hardware (T3 mostly).
fn hardware_change(s: &Session, rng: &mut Rng) -> Command {
    let ids = ["T3", "T3", "T3", "T6", "T2"];
    let Some(mut t) = s.line().structure(ids[rng.below(ids.len())]).cloned() else {
        return Command::Batch { commands: vec![] };
    };
    if t.strings.is_empty() || t.points.is_empty() {
        return moved(&t, rng.f() * 2.0 - 1.0);
    }
    let k = rng.below(t.strings.len());
    match rng.below(9) {
        0 => t.strings[k].strain = !t.strings[k].strain,
        1 => {
            t.strings.remove(k);
        }
        2 => t.strings[k].top = t.points[rng.below(t.points.len())].label.clone(),
        3 => {
            let p = rng.below(t.points.len());
            t.points[p].position[2] += rng.f() - 0.5;
        }
        4 => t.strings[k].insulator = Some(format!("i{}", rng.below(3))),
        5 => t
            .omit_sets
            .push(t.wires[rng.below(t.wires.len())].set.unwrap_or(1)),
        6 => t.routes = vec!["main".into()],
        7 => t.height_adjust += rng.f(),
        _ => return moved(&t, rng.f() * 2.0 - 1.0),
    }
    put_structure(&t)
}
