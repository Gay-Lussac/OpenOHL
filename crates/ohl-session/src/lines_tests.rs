// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Project lines ([`crate::lines`]): editable on their own, chosen per
//! session, brought up to date from another line, merged back by review.

use super::*;
use crate::lines::MAX_DESCRIPTION;
use crate::versions::{Choices, Side};
use std::path::PathBuf;

fn example() -> Line {
    Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap()
}

/// A project: ana admin, ben and cara designers, vic viewer.
fn project(name: &str) -> (PathBuf, Session) {
    let dir = std::env::temp_dir().join(format!("ohl-lines-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
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
    // Moving T3 changed its position, points and wire points, and the
    // branch's point tied to it: four conflicts.
    let keys: Vec<String> = s
        .bring_in_plan("main", &Choices::new())
        .unwrap()
        .0
        .into_iter()
        .map(|c| c.key)
        .collect();
    assert_eq!(
        keys,
        [
            "alignment_point:P2/position",
            "structure:T3/points",
            "structure:T3/position",
            "structure:T3/wires"
        ]
    );
    s.bring_in(
        "ben",
        "main",
        &Choices::from([
            ("structure:T3".to_owned(), Side::Ours),
            ("alignment_point:P2".to_owned(), Side::Ours),
        ]),
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
             UPDATE ols_meta SET value = '2' WHERE key = 'session_schema';
             CREATE TABLE line_snapshot (
                 name TEXT PRIMARY KEY, time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                 user TEXT NOT NULL, note TEXT NOT NULL, json TEXT NOT NULL);",
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
    // The frozen copies' table is gone once they are lines.
    let gone = !rusqlite::Connection::open(&path)
        .unwrap()
        .prepare("SELECT 1 FROM sqlite_master WHERE name = 'line_snapshot'")
        .unwrap()
        .exists([])
        .unwrap();
    assert!(gone);
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

#[test]
fn an_admin_rolls_a_line_back() {
    let (_path, mut s) = project("rollback");
    s.commit("ana", "start").unwrap();
    let start = s.head_revision().unwrap().unwrap();
    let t2 = x(&s, "main", "T2");
    shift(&mut s, "ben", "T2", 3.0).unwrap();
    s.commit("ben", "T2 east").unwrap();
    shift(&mut s, "ben", "T2", 1.0).unwrap();
    assert!(s.has_uncommitted("main").unwrap());
    // Admins only.
    assert!(s.roll_back_line("ben", "main", &start).is_err());
    s.create_line("ben", "phase 1", "main").unwrap();
    s.select_line("phase 1").unwrap();
    // Rolled back from another line: main's state is the start's again;
    // the work since is kept as revisions.
    let applied = s.roll_back_line("ana", "main", &start).unwrap();
    assert!(applied.is_some());
    assert_eq!(s.current_line(), "phase 1");
    assert_eq!(x(&s, "main", "T2"), t2);
    assert!(!s.has_uncommitted("main").unwrap());
    let messages: Vec<String> = s
        .revisions()
        .unwrap()
        .into_iter()
        .map(|r| r.message)
        .collect();
    assert!(
        messages.iter().any(|m| m.starts_with("Rolled back to")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.starts_with("Start of project line")),
        "{messages:?}"
    );
    // The phase keeps what it had.
    assert_eq!(x(&s, "phase 1", "T2"), t2 + 4.0);
}

#[test]
fn lines_have_descriptions() {
    let (path, mut s) = project("describe");
    s.create_line("ben", "phase 1", "main").unwrap();
    let long = "Towers 12 to 20 replaced.\n".repeat(200);
    s.set_line_description("ben", "phase 1", &long).unwrap();
    assert!(
        s.set_line_description("cara", "phase 1", "x").is_err(),
        "not hers"
    );
    assert!(
        s.set_line_description("ben", "main", "x").is_err(),
        "admins only"
    );
    s.set_line_description("ana", "main", "The line as built.")
        .unwrap();
    assert!(
        s.set_line_description("ben", "phase 1", &"x".repeat(MAX_DESCRIPTION + 1))
            .is_err()
    );
    s.rename_line("ben", "phase 1", "phase A").unwrap();
    s.checkpoint().unwrap();
    drop(s);
    let s = Session::open(&path).unwrap();
    let lines = s.lines().unwrap();
    assert_eq!(lines[0].description, "The line as built.");
    assert_eq!(
        lines[1].description,
        long.trim_end(),
        "kept through a rename"
    );
}

/// A protected line changes only through an admin (or an approved review):
/// a designer's edit is refused; making a line from it does not record an
/// admin's uncommitted change under the designer's name; its creator does
/// not delete it.
#[test]
fn a_protected_line_is_changed_by_admins_only() {
    let (_, mut s) = project("protected");
    s.commit("ana", "start").unwrap();
    s.protect_branch("ana", "main", true).unwrap();
    assert!(
        shift(&mut s, "ben", "T2", 1.0)
            .unwrap_err()
            .to_string()
            .contains("protected")
    );
    assert!(s.undo("ben").is_err());
    // The admin edits; a designer's new line would record that as a
    // revision of main: refused until the admin commits.
    shift(&mut s, "ana", "T2", 1.0).unwrap();
    assert!(
        s.create_line("ben", "phase 1", "main")
            .unwrap_err()
            .to_string()
            .contains("an admin commits them first")
    );
    s.commit("ana", "T2 moved").unwrap();
    s.create_line("ben", "phase 1", "main").unwrap();
    // Ben's own line, protected: not deleted by him; by an admin.
    s.protect_branch("ana", "phase 1", true).unwrap();
    assert!(
        s.delete_line("ben", "phase 1")
            .unwrap_err()
            .to_string()
            .contains("protected")
    );
    s.delete_line("ana", "phase 1").unwrap();
}

/// A library with a crafted name ("../…") that reached a revision (a
/// crafted copy of the project, a bundle) is refused when that revision is
/// restored: such names never come back as file names.
#[test]
fn crafted_library_names_are_not_restored() {
    let (path, s) = project("crafted-library");
    drop(s);
    {
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute(
            "INSERT INTO model_object (line, kind, id, ord, version, json) VALUES ('main', 'library', 'cables/../escaped.ohlw', 0, 1, '{\"id\":\"cables/../escaped.ohlw\",\"kind\":\"cables\",\"name\":\"../escaped.ohlw\",\"text\":\"x\"}')",
            [],
        )
        .unwrap();
    }
    let mut s = Session::open(&path).unwrap();
    let rev = s.commit("ana", "with a crafted library").unwrap();
    let err = s.roll_back_line("ana", "main", &rev.id).unwrap_err();
    assert!(err.to_string().contains("bad name"), "{err}");
}

/// Each project line has its library files: a change on one leaves the
/// others as they were; a new line starts with those of the line it is
/// made from.
#[test]
fn library_files_are_each_lines() {
    let (_path, mut s) = project("line-libraries");
    s.put_library_file("ana", "cables", "a.ohlw", "x = 1")
        .unwrap();
    s.create_line("ana", "phase 1", "main").unwrap();
    s.select_line("phase 1").unwrap();
    assert_eq!(s.library_files("cables").unwrap().len(), 1);
    s.put_library_file("ana", "cables", "a.ohlw", "x = 2")
        .unwrap();
    s.select_line("main").unwrap();
    assert_eq!(
        s.library_files("cables").unwrap(),
        vec![("a.ohlw".to_owned(), "x = 1".to_owned())]
    );
}
