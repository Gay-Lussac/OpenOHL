// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Project lines ([`crate::lines`]): editable on their own, chosen per
//! session, brought up to date from another line, merged back by review.

use super::*;
use crate::versions::{Choices, Side};
use std::path::PathBuf;

fn example() -> Line {
    Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ols.json"),
    )
    .unwrap()
}

/// A project: ana admin, ben and cara designers, vic viewer.
fn project(name: &str) -> (PathBuf, Session) {
    let dir = std::env::temp_dir().join(format!("ols-lines-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ols");
    std::fs::remove_file(&path).ok();
    let mut s = Session::open(&path).unwrap();
    s.import(&example(), "ana").unwrap();
    s.set_user("ana", "ben", Role::Designer).unwrap();
    s.set_user("ana", "cara", Role::Designer).unwrap();
    s.set_user("ana", "vic", Role::Viewer).unwrap();
    (path, s)
}

fn shift(s: &mut Session, user: &str, id: &str, dx: f64) -> Result<Applied, SessionError> {
    let p = s.line().structure(id).unwrap().position;
    s.apply(
        user,
        Command::MoveStructure {
            id: id.into(),
            position: [p[0] + dx, p[1], p[2]],
        },
    )
}

fn x(s: &Session, line: &str, id: &str) -> f64 {
    s.line_named(line).unwrap().structure(id).unwrap().position[0]
}

#[test]
fn lines_are_edited_on_their_own() {
    let (path, mut s) = project("own");
    assert_eq!(s.current_line(), "main");
    let x0 = x(&s, "main", "T2");
    assert!(
        s.create_line("vic", "phase 1", "main").is_err(),
        "viewers may not"
    );
    s.create_line("ben", "phase 1", "main").unwrap();
    assert!(
        s.create_line("ben", "phase 1", "main").is_err(),
        "names are unique"
    );
    // Made, not selected: still on main.
    assert_eq!(s.current_line(), "main");
    let names: Vec<String> = s.lines().unwrap().into_iter().map(|l| l.name).collect();
    assert_eq!(names, ["main", "phase 1"]);
    assert_eq!(s.lines().unwrap()[1].user, "ben");
    // An edit on the phase leaves main alone, and the other way round.
    s.select_line("phase 1").unwrap();
    shift(&mut s, "ben", "T2", 5.0).unwrap();
    assert_eq!(x(&s, "phase 1", "T2"), x0 + 5.0);
    assert_eq!(x(&s, "main", "T2"), x0);
    s.select_line("main").unwrap();
    shift(&mut s, "cara", "T2", -3.0).unwrap();
    assert_eq!(x(&s, "main", "T2"), x0 - 3.0);
    assert_eq!(x(&s, "phase 1", "T2"), x0 + 5.0);
    // Undo is per line: ben has nothing to undo on main.
    assert!(s.undo("ben").is_err());
    s.select_line("phase 1").unwrap();
    s.undo("ben").unwrap();
    assert_eq!(x(&s, "phase 1", "T2"), x0);
    assert_eq!(x(&s, "main", "T2"), x0 - 3.0);
    // A log entry of another line is not reversed from here.
    let main_entry = s
        .log()
        .unwrap()
        .into_iter()
        .rev()
        .find(|e| e.line == "main")
        .unwrap();
    assert!(s.rollback("ana", main_entry.seq).is_err());
    // Kept in the file: reopened, every line as it was.
    s.redo("ben").unwrap();
    s.checkpoint().unwrap();
    drop(s);
    let s = Session::open(&path).unwrap();
    assert_eq!(x(&s, "phase 1", "T2"), x0 + 5.0);
    assert_eq!(x(&s, "main", "T2"), x0 - 3.0);
    assert_eq!(s.current_line(), "main");
}

#[test]
fn who_may_delete_and_rename_lines() {
    let (_path, mut s) = project("rights");
    s.create_line("ben", "phase 1", "main").unwrap();
    s.create_line("cara", "phase 2", "main").unwrap();
    // Another designer's line: not cara's to delete or rename.
    assert!(s.delete_line("cara", "phase 1").is_err());
    assert!(s.rename_line("cara", "phase 1", "x").is_err());
    // The main line: admins only.
    assert!(s.delete_line("ben", "main").is_err());
    assert!(s.rename_line("ben", "main", "x").is_err());
    // The creator renames and deletes their own.
    s.rename_line("ben", "phase 1", "phase 1b").unwrap();
    assert!(s.is_line("phase 1b") && !s.is_line("phase 1"));
    s.delete_line("ben", "phase 1b").unwrap();
    assert!(!s.is_line("phase 1b"));
    // Not the line one works on; an admin deletes any other.
    s.select_line("phase 2").unwrap();
    assert!(s.delete_line("cara", "phase 2").is_err());
    s.delete_line("ana", "main").unwrap();
    assert_eq!(s.lines().unwrap().len(), 1);
    assert!(s.delete_line("ana", "phase 2").is_err(), "the only line");
}

#[test]
fn a_phase_brings_in_main_and_goes_back_through_a_review() {
    let (_path, mut s) = project("bring");
    s.commit("ana", "start").unwrap();
    s.create_line("ben", "phase 1", "main").unwrap();
    let (t2, t5, t3) = (
        x(&s, "main", "T2"),
        x(&s, "main", "T5"),
        x(&s, "main", "T3"),
    );
    // Main corrected after the phase was made (uncommitted).
    shift(&mut s, "cara", "T2", 1.0).unwrap();
    s.select_line("phase 1").unwrap();
    shift(&mut s, "ben", "T5", 2.0).unwrap();
    // Not by itself: the phase is a copy.
    assert_eq!(x(&s, "phase 1", "T2"), t2);
    let out = s.bring_in("ben", "main", &Choices::new()).unwrap();
    assert!(out.applied.is_some());
    assert_eq!(x(&s, "phase 1", "T2"), t2 + 1.0, "main's correction");
    assert_eq!(x(&s, "phase 1", "T5"), t5 + 2.0, "the phase's own");
    assert_eq!(x(&s, "main", "T5"), t5, "main untouched");
    // The same structure changed on both sides: a conflict to settle.
    shift(&mut s, "ben", "T3", 4.0).unwrap();
    s.select_line("main").unwrap();
    shift(&mut s, "cara", "T3", -4.0).unwrap();
    s.select_line("phase 1").unwrap();
    let revisions = s.revisions().unwrap().len();
    let e = s.bring_in("ben", "main", &Choices::new()).unwrap_err();
    assert!(e.to_string().contains("conflict"), "{e}");
    // Refused: nothing recorded (a server and its replicas stay alike).
    assert_eq!(s.revisions().unwrap().len(), revisions);
    // Moving T3 changed its position and attachments: two conflicts,
    // settled at once for the whole structure.
    let keys: Vec<String> = s
        .bring_in_plan("main", &Choices::new())
        .unwrap()
        .0
        .into_iter()
        .map(|c| c.key)
        .collect();
    assert_eq!(keys, ["structure:T3/attachments", "structure:T3/position"]);
    s.bring_in(
        "ben",
        "main",
        &Choices::from([("structure:T3".to_owned(), Side::Ours)]),
    )
    .unwrap();
    assert_eq!(x(&s, "phase 1", "T3"), t3 + 4.0, "the phase's choice");
    // Back into main: submitted, approved by the admin, merged — while
    // the reviewer works on another line.
    // Bringing in recorded both lines: nothing left to commit.
    assert!(s.commit("ben", "phase 1 ready").is_err());
    let id = s
        .submit_review("ben", "phase 1", "main", &[], "phase 1")
        .unwrap();
    s.decide_review("ana", id, true, "").unwrap();
    // The phase holds main's history (brought in): a fast-forward.
    let out = s.merge_review("ana", id, None, &Choices::new()).unwrap();
    assert_eq!(out.kind, crate::versions::MergeKind::FastForward);
    assert!(out.applied.is_some());
    assert_eq!(
        s.current_line(),
        "phase 1",
        "the reviewer stays on their line"
    );
    assert_eq!(x(&s, "main", "T5"), t5 + 2.0);
    assert_eq!(x(&s, "main", "T3"), t3 + 4.0);
}

#[test]
fn files_of_schema_2_become_lines() {
    let (path, s) = project("migrate");
    drop(s);
    // Back to schema 2: no line columns, a frozen project line saved.
    {
        let c = rusqlite::Connection::open(&path).unwrap();
        let mut saved = example();
        saved.structures[1].position[0] += 7.0;
        c.execute_batch(
            "CREATE TABLE m2 AS SELECT kind, id, ord, version, json FROM model_object;
             DROP TABLE model_object; ALTER TABLE m2 RENAME TO model_object;
             ALTER TABLE oplog DROP COLUMN line;
             DROP TABLE spatial_index;
             DELETE FROM vc_ref WHERE name <> 'main';
             UPDATE ols_meta SET value = '2' WHERE key = 'session_schema';",
        )
        .unwrap();
        c.execute(
            "INSERT INTO line_snapshot (name, user, note, json) VALUES ('as built', 'ben', 'old', ?1)",
            rusqlite::params![serde_json::to_string(&saved).unwrap()],
        )
        .unwrap();
    }
    let mut s = Session::open(&path).unwrap();
    assert_eq!(s.current_line(), "main");
    assert_eq!(s.line(), &example());
    let lines = s.lines().unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(
        (lines[1].name.as_str(), lines[1].user.as_str()),
        ("as built", "ben")
    );
    let id = &example().structures[1].id;
    assert_eq!(
        x(&s, "as built", id),
        example().structures[1].position[0] + 7.0
    );
    // Editable now, and undone on its own.
    s.select_line("as built").unwrap();
    shift(&mut s, "ben", id, 1.0).unwrap();
    s.undo("ben").unwrap();
    assert!(
        s.log()
            .unwrap()
            .iter()
            .all(|e| e.line == "main" || e.line == "as built")
    );
}

#[test]
fn a_session_follows_its_line_renamed_by_another() {
    let (_path, mut s) = project("follow");
    s.create_line("ben", "phase 1", "main").unwrap();
    s.select_line("phase 1").unwrap();
    // Another user's rename, run from their line (as a replica does).
    s.with_line("main", |s| s.rename_line("ben", "phase 1", "phase A"))
        .unwrap();
    assert_eq!(s.current_line(), "phase A");
    // Deleted while worked on by someone else: the session is moved.
    s.with_line("main", |s| s.delete_line("ana", "phase A"))
        .unwrap();
    assert_eq!(s.current_line(), "main");
}

#[test]
fn lines_started_without_any_commit_share_their_start() {
    let (_path, mut s) = project("start");
    s.create_line("ben", "phase 1", "main").unwrap();
    let t2 = x(&s, "main", "T2");
    shift(&mut s, "cara", "T2", 2.0).unwrap();
    s.select_line("phase 1").unwrap();
    s.apply(
        "ben",
        Command::DeleteStructure {
            id: "T7".into(),
            confirm: true,
        },
    )
    .unwrap();
    let (conflicts, problem) = s.bring_in_plan("main", &Choices::new()).unwrap();
    assert!(conflicts.is_empty() && problem.is_none(), "{conflicts:?}");
    s.bring_in("ben", "main", &Choices::new()).unwrap();
    assert_eq!(x(&s, "phase 1", "T2"), t2 + 2.0);
    assert!(
        s.line().structure("T7").is_none(),
        "the phase's deletion kept"
    );
}
