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
            },
        )
        .unwrap_err();
    assert!(matches!(err, SessionError::Refused(_)), "{err}");
    assert_eq!(s.export(), before);
    // A viewer may not edit.
    assert!(
        s.apply(
            "vic",
            Command::Delete {
                kind: Kind::Jumper,
                id: "J-T3-branch".into()
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
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("differs"), "{err}");
    s.apply(
        "ben",
        Command::Delete {
            kind: Kind::Jumper,
            id: "J2".into(),
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
