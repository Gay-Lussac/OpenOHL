// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use std::path::PathBuf;

fn example() -> Line {
    Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ols.json"),
    )
    .unwrap()
}

fn project(name: &str) -> (PathBuf, Session) {
    let dir = std::env::temp_dir().join(format!("ols-session-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ols");
    std::fs::remove_file(&path).ok();
    let mut s = Session::open(&path).unwrap();
    s.import(&example(), "ana").unwrap();
    s.set_user("ana", "ben", Role::Designer).unwrap();
    s.set_user("ana", "vic", Role::Viewer).unwrap();
    (path, s)
}

fn position(s: &Session, id: &str) -> Point {
    s.line().structure(id).unwrap().position
}

#[test]
fn import_export_round_trip_and_reopen() {
    let (path, s) = project("roundtrip");
    assert_eq!(s.export(), example());
    assert_eq!(s.role("ana").unwrap(), Some(Role::Admin));
    drop(s);
    let again = Session::open(&path).unwrap();
    assert_eq!(again.export(), example());
    assert_eq!(again.log().unwrap()[0].kind, "import");
}

#[test]
fn commands_are_validated_and_report_dirty_sections() {
    let (_, mut s) = project("validate");
    let p = position(&s, "T6");
    let applied = s
        .apply(
            "ben",
            Command::MoveStructure {
                id: "T6".into(),
                position: [p[0] + 5.0, p[1], p[2]],
            },
        )
        .unwrap();
    // T6 is on the branch only: S3 to recalculate.
    assert_eq!(applied.sections, vec!["S3".to_owned()]);
    let t6 = s.line().structure("T6").unwrap();
    assert_eq!(t6.position[0], p[0] + 5.0);
    // Attachments moved with it.
    assert_eq!(
        t6.attachments[0].insulator_point[0] - t6.position[0],
        example().structure("T6").unwrap().attachments[0].insulator_point[0] - p[0]
    );
    // Deleting a structure used by sections makes the line invalid: refused,
    // nothing changed.
    let before = s.export();
    let err = s
        .apply(
            "ben",
            Command::Delete {
                kind: Kind::Structure,
                id: "T2".into(),
                confirm: false,
            },
        )
        .unwrap_err();
    // It takes sections with it: a confirmation is asked, nothing changes.
    assert!(matches!(err, SessionError::NeedsConfirmation(_)), "{err}");
    assert_eq!(s.export(), before);
    // A viewer may not edit.
    assert!(
        s.apply(
            "vic",
            Command::Delete {
                kind: Kind::Jumper,
                id: "J-T3-branch".into(),
                confirm: false,
            }
        )
        .unwrap_err()
        .to_string()
        .contains("viewer")
    );
    // Batch: all or nothing.
    let stringing = Stringing::Tension {
        horizontal_tension: 24_000.0,
        temperature: 10.0,
    };
    let bad_batch = Command::Batch {
        commands: vec![
            Command::SetStringing {
                section: "S1".into(),
                stringing: stringing.clone(),
            },
            Command::Delete {
                kind: Kind::Structure,
                id: "T1".into(),
                confirm: false,
            },
        ],
    };
    assert!(s.apply("ben", bad_batch).is_err());
    assert_eq!(
        s.line().sections[0].stringing,
        example().sections[0].stringing
    );
}

#[test]
fn undo_is_per_user_and_refuses_to_overwrite_others() {
    let (_, mut s) = project("undo");
    let p = position(&s, "T4");
    let moved = |dx: f64| Command::MoveStructure {
        id: "T4".into(),
        position: [p[0] + dx, p[1], p[2]],
    };
    s.apply("ana", moved(1.0)).unwrap();
    s.apply(
        "ben",
        Command::SetStringing {
            section: "S2".into(),
            stringing: Stringing::Tension {
                horizontal_tension: 15_000.0,
                temperature: 15.0,
            },
        },
    )
    .unwrap();
    // Ana undoes her move; Ben's change stays.
    s.undo("ana").unwrap();
    assert_eq!(position(&s, "T4"), p);
    assert!(matches!(
        s.line().sections[1].stringing,
        Stringing::Tension { .. }
    ));
    // Redo brings the move back; a second redo has nothing to do.
    s.redo("ana").unwrap();
    assert_eq!(position(&s, "T4")[0], p[0] + 1.0);
    assert!(s.redo("ana").is_err());
    // Ben moves T4 further: Ana's undo would overwrite his change — refused.
    s.apply("ben", moved(3.0)).unwrap();
    let err = s.undo("ana").unwrap_err().to_string();
    assert!(err.contains("changed after"), "{err}");
    assert_eq!(position(&s, "T4")[0], p[0] + 3.0);
    // Ben undoes his move, then Ana can undo hers.
    s.undo("ben").unwrap();
    s.undo("ana").unwrap();
    assert_eq!(position(&s, "T4"), p);
}

#[test]
fn admin_rollback() {
    let (_, mut s) = project("rollback");
    let start = s.log().unwrap().last().unwrap().seq;
    let p = position(&s, "T5");
    let ben_move = s
        .apply(
            "ben",
            Command::MoveStructure {
                id: "T5".into(),
                position: [p[0], p[1] + 2.0, p[2]],
            },
        )
        .unwrap();
    // Only admins roll back others' commands.
    assert!(s.rollback("ben", ben_move.seq).is_err());
    s.rollback("ana", ben_move.seq).unwrap();
    assert_eq!(position(&s, "T5"), p);
    // Several edits by both, then everything after the import is rolled back.
    for (user, dy) in [("ben", 1.0), ("ana", 2.0), ("ben", 3.0)] {
        s.apply(
            user,
            Command::MoveStructure {
                id: "T5".into(),
                position: [p[0], p[1] + dy, p[2]],
            },
        )
        .unwrap();
    }
    s.undo("ben").unwrap();
    s.rollback_after("ana", start).unwrap();
    assert_eq!(s.export(), example());
    let log = s.log().unwrap();
    let last = log.last().unwrap();
    assert_eq!(last.kind, "rollback");
    // Every entry between the import and the final rollback is marked as
    // reversed (by it, or earlier by an undo or a rollback).
    assert!(
        log[1..log.len() - 1].iter().all(|e| e.undone_by.is_some()),
        "{log:#?}"
    );
    // Nothing left to roll back.
    assert!(s.rollback_after("ana", start).unwrap().is_none());
}

#[test]
fn put_and_delete_objects() {
    let (_, mut s) = project("put");
    let mut j = example().jumpers[0].clone();
    j.id = "J2".into();
    j.sag = Some(2.0);
    s.apply(
        "ben",
        Command::Put {
            kind: Kind::Jumper,
            id: "J2".into(),
            value: serde_json::to_value(&j).unwrap(),
            ord: None,
        },
    )
    .unwrap();
    assert_eq!(s.line().jumpers.len(), 2);
    assert_eq!(s.version(Kind::Jumper, "J2"), 1);
    // The id inside must match.
    let err = s
        .apply(
            "ben",
            Command::Put {
                kind: Kind::Jumper,
                id: "J3".into(),
                value: serde_json::to_value(&j).unwrap(),
                ord: None,
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("differs"), "{err}");
    s.apply(
        "ben",
        Command::Delete {
            kind: Kind::Jumper,
            id: "J2".into(),
            confirm: false,
        },
    )
    .unwrap();
    s.undo("ben").unwrap();
    assert_eq!(s.line().jumpers.len(), 2);
    // Commands are plain JSON.
    let c: Command =
        serde_json::from_str(r#"{"op": "delete", "kind": "jumper", "id": "J2"}"#).unwrap();
    s.apply("ben", c).unwrap();
    assert_eq!(s.line().jumpers.len(), 1);
}

#[test]
fn deleting_a_structure_lists_what_goes_with_it_and_is_logged() {
    let (_, mut s) = project("delete");
    let delete = |confirm| Command::DeleteStructure {
        id: "T3".into(),
        confirm,
    };
    // Without confirmation: refused, with the full list, nothing changed.
    let err = s.apply("ben", delete(false)).unwrap_err();
    let SessionError::NeedsConfirmation(impact) = &err else {
        panic!("{err}");
    };
    assert_eq!(impact.sections, vec!["S1", "S2", "S3"]);
    assert_eq!(impact.jumpers, vec!["J-T3-branch"]);
    assert_eq!(impact.attachments.len(), 3);
    // T3 is the Y pylon: main keeps 4 structures, the branch keeps T6–T7.
    assert_eq!(impact.routes_changed, vec!["main", "branch"]);
    assert!(impact.routes_deleted.is_empty());
    assert!(err.to_string().contains("S3") && err.to_string().contains("J-T3-branch"));
    assert_eq!(s.export(), example());
    // A viewer gets the permission error, not the list.
    assert!(!matches!(
        s.apply("vic", delete(false)),
        Err(SessionError::NeedsConfirmation(_))
    ));

    // Confirmed: everything goes in one logged entry.
    let applied = s.apply("ben", delete(true)).unwrap();
    assert!(s.line().structure("T3").is_none());
    assert!(s.line().sections.is_empty() && s.line().jumpers.is_empty());
    assert!(
        s.line()
            .routes
            .iter()
            .all(|r| !r.structures.contains(&"T3".to_owned()))
    );
    let entry = s
        .log()
        .unwrap()
        .into_iter()
        .find(|e| e.seq == applied.seq)
        .unwrap();
    let note = entry.note.unwrap();
    assert!(
        note.contains("delete structure T3") && note.contains("S1, S2, S3"),
        "{note}"
    );
    assert_eq!(entry.user, "ben");

    // Undo restores all of it.
    let undo = s.undo("ben").unwrap();
    assert_eq!(s.export(), example());
    let note = s
        .log()
        .unwrap()
        .into_iter()
        .find(|e| e.seq == undo.seq)
        .unwrap()
        .note
        .unwrap();
    assert!(
        note.starts_with(&format!("reverses {}", applied.seq)),
        "{note}"
    );
}

#[test]
fn deleting_the_last_structures_of_a_route_deletes_the_route() {
    let (_, mut s) = project("route");
    // The branch is T3–T6–T7; without T7 and T6 it would have one structure.
    s.apply(
        "ben",
        Command::DeleteStructure {
            id: "T7".into(),
            confirm: true,
        },
    )
    .unwrap();
    let impact = s.structure_deletion("T6").unwrap();
    assert_eq!(impact.routes_deleted, vec!["branch"]);
    s.apply(
        "ben",
        Command::DeleteStructure {
            id: "T6".into(),
            confirm: true,
        },
    )
    .unwrap();
    assert_eq!(s.line().routes.len(), 1);
}

#[test]
fn deleting_a_section_takes_its_jumpers() {
    let (_, mut s) = project("section");
    let del = |confirm| Command::Delete {
        kind: Kind::Section,
        id: "S3".into(),
        confirm,
    };
    let Err(SessionError::NeedsConfirmation(i)) = s.apply("ben", del(false)) else {
        panic!("confirmation expected");
    };
    assert_eq!(i.jumpers, vec!["J-T3-branch"]);
    assert!(i.sections.is_empty() && i.attachments.is_empty());
    s.apply("ben", del(true)).unwrap();
    assert!(s.line().jumpers.is_empty() && s.line().sections.len() == 2);
    // A section without jumpers goes without confirmation (still logged).
    let a = s
        .apply(
            "ben",
            Command::Delete {
                kind: Kind::Section,
                id: "S2".into(),
                confirm: false,
            },
        )
        .unwrap();
    let note = s
        .log()
        .unwrap()
        .into_iter()
        .find(|e| e.seq == a.seq)
        .unwrap()
        .note;
    assert_eq!(note.as_deref(), Some("delete section S2"));
    s.undo("ben").unwrap();
    s.undo("ben").unwrap();
    assert_eq!(s.export(), example());
}

#[test]
fn alignment_points_follow_their_structure_until_detached() {
    let (_, mut s) = project("alignment");
    // Points under the branch's structures, tied.
    s.apply(
        "ben",
        Command::AlignUnderStructures {
            route: "branch".into(),
        },
    )
    .unwrap();
    let branch = |s: &Session| {
        s.line()
            .routes
            .iter()
            .find(|r| r.id == "branch")
            .unwrap()
            .clone()
    };
    assert_eq!(branch(&s).alignment.len(), 3);
    assert_eq!(branch(&s).alignment[1].structure.as_deref(), Some("T6"));
    // Moving T6 moves its point.
    let p = position(&s, "T6");
    let moved = [p[0] + 4.0, p[1], p[2]];
    s.apply(
        "ben",
        Command::MoveStructure {
            id: "T6".into(),
            position: moved,
        },
    )
    .unwrap();
    assert_eq!(branch(&s).alignment[1].position, moved);
    // Moving the point detaches it; then T6 moves alone.
    let a = s
        .apply(
            "ben",
            Command::MoveAlignmentPoint {
                route: "branch".into(),
                index: 1,
                position: [moved[0] + 10.0, moved[1], moved[2]],
            },
        )
        .unwrap();
    assert!(branch(&s).alignment[1].structure.is_none());
    let note = s
        .log()
        .unwrap()
        .into_iter()
        .find(|e| e.seq == a.seq)
        .unwrap()
        .note
        .unwrap();
    assert!(note.contains("detached from T6"), "{note}");
    s.apply(
        "ben",
        Command::MoveStructure {
            id: "T6".into(),
            position: p,
        },
    )
    .unwrap();
    assert_eq!(branch(&s).alignment[1].position[0], moved[0] + 10.0);
    // Tie it again: back under T6.
    s.apply(
        "ben",
        Command::TieAlignmentPoint {
            route: "branch".into(),
            index: 1,
            structure: "T6".into(),
        },
    )
    .unwrap();
    assert_eq!(branch(&s).alignment[1].position, p);
    // A tied point away from its structure is invalid.
    let mut r = branch(&s);
    r.alignment[1].position[0] += 1.0;
    let err = s
        .apply(
            "ben",
            Command::Put {
                kind: Kind::Route,
                id: "branch".into(),
                value: serde_json::to_value(&r).unwrap(),
                ord: None,
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("tied to T6"), "{err}");
    // Deleting T7: its point is detached (listed), the branch keeps T3–T6.
    let impact = s.structure_deletion("T7").unwrap();
    assert_eq!(impact.alignment_detached, vec!["branch #2"]);
    s.apply(
        "ben",
        Command::DeleteStructure {
            id: "T7".into(),
            confirm: true,
        },
    )
    .unwrap();
    let b = branch(&s);
    assert_eq!(b.structures, vec!["T3", "T6"]);
    assert_eq!(b.alignment.len(), 3);
    assert!(b.alignment[2].structure.is_none());
    // Files: free points as arrays, tied ones as objects.
    let text = serde_json::to_string(&b.alignment).unwrap();
    assert!(
        text.starts_with(r#"[{"position""#) && text.contains(r#""structure":"T3""#),
        "{text}"
    );
    // T3 and T6 tied (T6 was re-tied above), T7's former point free.
    assert!(text.ends_with(r#"},[640.0,560.0,496.0]]"#), "{text}");
}

/// What a query must return: every object whose bounds touch the area.
fn brute_force(s: &Session, area: &Bounds) -> Vec<(Kind, String)> {
    spatial::object_bounds(s.line())
        .into_iter()
        .filter(|(_, b)| b.intersects(area))
        .map(|(k, _)| k)
        .collect()
}

fn queried(s: &Session, area: &Bounds) -> Vec<(Kind, String)> {
    s.objects_in(area)
        .unwrap()
        .into_iter()
        .map(|(k, id, _)| (k, id))
        .collect()
}

#[test]
fn spatial_index_follows_every_command() {
    let (path, mut s) = project("spatial");
    let areas = |s: &Session| {
        // A sweep of small and large areas over the line and around it.
        let mut v = Vec::new();
        for x in (-200..1400).step_by(150) {
            for y in (-200..700).step_by(150) {
                let (x, y) = (x as f64, y as f64);
                v.push(Bounds::new([x, y], [x + 20.0, y + 20.0]));
                v.push(Bounds::new([x, y], [x + 400.0, y + 90.0]));
            }
        }
        for a in &v {
            assert_eq!(queried(s, a), brute_force(s, a), "{a:?}");
        }
    };
    areas(&s);
    // Near T6 (640, 280): T6, the branch section S3, the branch route.
    let near_t6 = Bounds::new([630.0, 270.0], [650.0, 290.0]);
    assert_eq!(
        queried(&s, &near_t6),
        [
            (Kind::Route, "branch".to_owned()),
            (Kind::Structure, "T6".to_owned()),
            (Kind::Section, "S3".to_owned()),
        ]
    );
    // Home tiles: pylons at level 0, the 1.25 km main sections higher.
    assert_eq!(
        s.spatial_entry(Kind::Structure, "T6").unwrap().tile.level,
        0
    );
    assert_eq!(s.spatial_entry(Kind::Section, "S1").unwrap().tile.level, 3);

    // Moving T6 away moves its section's bounds and the route's with it.
    let p = position(&s, "T6");
    s.apply(
        "ben",
        Command::MoveStructure {
            id: "T6".into(),
            position: [p[0] + 300.0, p[1], p[2]],
        },
    )
    .unwrap();
    // T6 has left; S3 and the route still touch by their bounds (now
    // x 640–940): queries select by bounds, callers test exact geometry.
    assert_eq!(
        queried(&s, &near_t6),
        [
            (Kind::Route, "branch".to_owned()),
            (Kind::Section, "S3".to_owned()),
        ]
    );
    let moved = Bounds::new([930.0, 270.0], [950.0, 290.0]);
    assert!(queried(&s, &moved).contains(&(Kind::Section, "S3".to_owned())));
    areas(&s);
    // Undo, delete: still in step with the line.
    s.undo("ben").unwrap();
    areas(&s);
    s.apply(
        "ana",
        Command::Delete {
            kind: Kind::Section,
            id: "S3".into(),
            confirm: true,
        },
    )
    .unwrap();
    assert!(s.spatial_entry(Kind::Section, "S3").is_none());
    areas(&s);

    // The index is in the file: reopened, and rebuilt when missing.
    drop(s);
    let s = Session::open(&path).unwrap();
    areas(&s);
    s.conn.execute("DELETE FROM spatial_index", []).unwrap();
    drop(s);
    let s = Session::open(&path).unwrap();
    assert!(!s.objects_in(&near_t6).unwrap().is_empty());
    areas(&s);
    assert_eq!(s.grid().size, Grid::DEFAULT_SIZE);
    let counted: usize = s.home_tiles().values().flat_map(|m| m.values()).sum();
    assert_eq!(counted, spatial::object_bounds(s.line()).len());
}

#[test]
fn deleting_a_string_takes_its_sections_and_jumpers() {
    let (_, mut s) = project("string");
    let del = |confirm| Command::DeleteAttachment {
        structure: "T3".into(),
        set: 2,
        phase: 1,
        confirm,
    };
    let Err(SessionError::NeedsConfirmation(i)) = s.apply("ben", del(false)) else {
        panic!("confirmation expected");
    };
    assert_eq!(i.what(), "structure T3 string 2:1");
    assert_eq!(i.sections, vec!["S3"]);
    assert_eq!(i.jumpers, vec!["J-T3-branch"]);
    let applied = s.apply("ben", del(true)).unwrap();
    let t3 = s.line().structure("T3").unwrap();
    assert!(!t3.attachments.iter().any(|a| (a.set, a.phase) == (2, 1)));
    assert!(
        t3.omit.is_empty(),
        "an explicit string is removed, not omitted"
    );
    assert!(s.line().sections.iter().all(|x| x.id != "S3"));
    assert!(s.line().jumpers.is_empty());
    let note = s
        .log()
        .unwrap()
        .into_iter()
        .find(|e| e.seq == applied.seq)
        .unwrap()
        .note
        .unwrap();
    assert!(note.contains("string 2:1") && note.contains("S3"), "{note}");
    s.undo("ben").unwrap();
    assert_eq!(s.export(), example());
    // A string nothing hangs on goes without a confirmation; one that is
    // not there is refused.
    s.apply("ben", del(true)).unwrap();
    assert!(s.apply("ben", del(false)).is_err());
}

#[test]
fn a_typed_structure_leaves_out_a_string_of_its_type() {
    let dir = std::env::temp_dir().join(format!("ols-session-{}-typed", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ols");
    std::fs::remove_file(&path).ok();
    let ex = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
    let line = Line::load(&ex.join("synthetic-line-typed.ols.json")).unwrap();
    let mut s = Session::open(&path).unwrap();
    s.import(&line, "ana").unwrap();
    s.apply(
        "ana",
        Command::DeleteAttachment {
            structure: "T3".into(),
            set: 2,
            phase: 1,
            confirm: true,
        },
    )
    .unwrap();
    let t3 = s.line().structure("T3").unwrap();
    assert_eq!(t3.omit, vec![[2, 1]]);
}

#[test]
fn renaming_keeps_the_place_and_what_refers_to_it_follows() {
    let (_, mut s) = project("rename");
    let order =
        |s: &Session| -> Vec<String> { s.line().structures.iter().map(|x| x.id.clone()).collect() };
    let before = order(&s);
    s.apply(
        "ben",
        Command::Rename {
            kind: Kind::Structure,
            id: "T3".into(),
            new_id: "Y3".into(),
        },
    )
    .unwrap();
    let after = order(&s);
    assert_eq!(after[2], "Y3", "same place: {after:?}");
    assert_eq!(after.len(), before.len());
    let l = s.line();
    assert!(
        l.sections
            .iter()
            .all(|x| x.points.iter().all(|p| p.structure != "T3"))
    );
    assert!(l.sections[0].points.iter().any(|p| p.structure == "Y3"));
    assert_eq!(l.jumpers[0].from.structure, "Y3");
    assert!(
        l.routes
            .iter()
            .all(|r| !r.structures.contains(&"T3".to_owned()))
    );
    assert!(
        s.apply(
            "ben",
            Command::Rename {
                kind: Kind::Structure,
                id: "Y3".into(),
                new_id: "T2".into(),
            },
        )
        .is_err(),
        "taken"
    );
    s.undo("ben").unwrap();
    assert_eq!(s.export(), example());
    // A section, a string, a system.
    s.apply(
        "ben",
        Command::Rename {
            kind: Kind::Section,
            id: "S3".into(),
            new_id: "branch".into(),
        },
    )
    .unwrap();
    assert_eq!(s.line().sections[2].id, "branch");
    s.apply(
        "ben",
        Command::RenameAttachment {
            structure: "T3".into(),
            set: 2,
            phase: 1,
            new_set: 5,
            new_phase: 1,
        },
    )
    .unwrap();
    let p = &s.line().sections[2].points[0];
    assert_eq!((p.structure.as_str(), p.set, p.phase), ("T3", 5, 1));
    assert_eq!(s.line().jumpers[0].from.set, 5);
    let sys = s.line().systems[0].id.clone();
    s.apply(
        "ben",
        Command::RenameSystem {
            id: sys.clone(),
            new_id: "main-220".into(),
        },
    )
    .unwrap();
    assert_eq!(s.line().systems[0].id, "main-220");
    assert!(
        s.line()
            .sections
            .iter()
            .all(|x| x.system.as_deref() != Some(sys.as_str()))
    );
}

#[test]
fn settings_are_kept_with_the_project() {
    let (path, s) = project("settings");
    assert_eq!(s.setting("app.weather").unwrap(), None);
    s.set_setting("app.weather", Some("/x/cases.olswc"))
        .unwrap();
    s.set_setting("app.weather", Some("/y/cases.olswc"))
        .unwrap();
    drop(s);
    let again = Session::open(&path).unwrap();
    assert_eq!(
        again.setting("app.weather").unwrap().as_deref(),
        Some("/y/cases.olswc")
    );
    again.set_setting("app.weather", None).unwrap();
    assert_eq!(again.setting("app.weather").unwrap(), None);
    assert_eq!(again.export(), example(), "not part of the line");
}

#[test]
fn project_lines_are_saved_compared_and_made_the_working_line() {
    let (path, mut s) = project("snapshots");
    s.save_snapshot("ben", "as built", "before the works")
        .unwrap();
    assert!(
        s.save_snapshot("ben", "as built", "").is_err(),
        "names are unique"
    );
    assert!(
        s.save_snapshot("vic", "viewer", "").is_err(),
        "viewers may not"
    );
    // The working line changes; the saved one does not.
    s.apply(
        "ben",
        Command::DeleteStructure {
            id: "T7".into(),
            confirm: true,
        },
    )
    .unwrap();
    assert_eq!(s.snapshot("as built").unwrap(), example());
    let list = s.snapshots().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(
        (list[0].name.as_str(), list[0].user.as_str()),
        ("as built", "ben")
    );
    // Made the working line again: one entry, undone at once.
    let back = s.snapshot("as built").unwrap();
    s.apply(
        "ben",
        Command::ReplaceLine {
            line: Box::new(back),
        },
    )
    .unwrap();
    assert_eq!(s.export(), example());
    s.undo("ben").unwrap();
    assert!(s.line().structure("T7").is_none());
    s.checkpoint().unwrap();
    drop(s);
    let again = Session::open(&path).unwrap();
    assert_eq!(again.snapshots().unwrap().len(), 1);
    again.delete_snapshot("ben", "as built").unwrap();
    assert!(again.snapshots().unwrap().is_empty());
}
