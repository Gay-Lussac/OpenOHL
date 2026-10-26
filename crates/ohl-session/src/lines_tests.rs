// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

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

/// A library with a crafted name ("../…") put straight into a project
/// file is never handed out as a file name, never committed into a
/// revision, and restoring a revision takes it away (security review
/// 2026-10-26: names are checked wherever library entries are read).
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
    assert!(s.library_files("cables").unwrap().is_empty());
    let rev = s.commit("ana", "with a crafted library").unwrap();
    s.roll_back_line("ana", "main", &rev.id).unwrap();
    assert!(
        s.version(Kind::Library, "cables/../escaped.ohlw") == 0,
        "the crafted object is gone"
    );
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

/// A line as the session holds it: its objects (place, version, JSON)
/// and line.
type Held = (BTreeMap<(Kind, String), Object>, Line);

/// Every line as the session holds it.
fn held(s: &Session) -> BTreeMap<String, Held> {
    let mut out: BTreeMap<_, _> = s
        .others
        .iter()
        .map(|(name, l)| (name.clone(), (l.objects.clone(), l.line.clone())))
        .collect();
    out.insert(s.current.clone(), (s.objects.clone(), s.line.clone()));
    out
}

/// Opened again, the project holds every line exactly as before.
fn reopens_alike(path: &Path, s: Session) -> Session {
    let before = held(&s);
    let current = s.current_line().to_owned();
    drop(s);
    let mut s = Session::open(path).unwrap();
    s.select_line(&current).unwrap();
    assert_eq!(held(&s), before);
    s
}

/// Rows of `model_object` of line `line`: objects and tombstones.
fn rows(path: &Path, line: &str) -> (i64, i64) {
    let c = rusqlite::Connection::open(path).unwrap();
    let count = |gone: bool| -> i64 {
        c.query_row(
            "SELECT count(*) FROM model_object WHERE line = ?1 AND gone = ?2",
            rusqlite::params![line, gone],
            |r| r.get(0),
        )
        .unwrap()
    };
    (count(false), count(true))
}

/// Rows of `table` of line `line`, whatever its columns.
fn rows_of(path: &Path, table: &str, line: &str) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(
            &format!("SELECT count(*) FROM {table} WHERE line = ?1"),
            [line],
            |r| r.get(0),
        )
        .unwrap()
}

/// A new line keeps only what differs from the revision it starts from:
/// nothing when it starts (but library files placed otherwise among the
/// objects); its edits, deletions (tombstones) and undos on their own;
/// the line it came from renamed and deleted, the line brought up to
/// date, rolled back, carried as a bundle — every time read back exactly.
#[test]
fn new_lines_keep_only_their_differences() {
    let (path, mut s) = project("delta");
    // Library files put out of name order: placed otherwise than a
    // revision keeps them.
    s.put_library_file("ana", "cables", "b.ohlw", "b = 1")
        .unwrap();
    s.put_library_file("ana", "cables", "a.ohlw", "a = 1")
        .unwrap();
    shift(&mut s, "ana", "T2", 1.0).unwrap();
    let main_rows = rows(&path, "main");
    s.create_line("ben", "phase 1", "main").unwrap();
    // The start recorded on main; the line's base is that revision.
    let start = s.branch_revision("main").unwrap().unwrap();
    assert_eq!(s.branch_revision("phase 1").unwrap(), Some(start.clone()));
    assert_eq!(
        crate::lines::line_base(&s.conn, "phase 1").unwrap(),
        Some(start.clone())
    );
    // The two library files: their places only (no JSON).
    assert_eq!(rows(&path, "phase 1"), (0, 0));
    let places: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT count(*) FROM model_object WHERE line = 'phase 1' AND gone = 2 AND json = 'null'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(places, 2, "the two library files' places");
    assert_eq!(
        rows_of(&path, "spatial_index", "phase 1"),
        0,
        "indexed when worked on"
    );
    assert_eq!(rows(&path, "main"), main_rows, "main as it was");
    assert_eq!(s.line_named("phase 1").unwrap(), s.line());
    let mut s = reopens_alike(&path, s);
    // Worked on: its index written, its edits its own.
    s.select_line("phase 1").unwrap();
    assert_eq!(
        rows_of(&path, "spatial_index", "phase 1"),
        rows_of(&path, "spatial_index", "main")
    );
    assert_eq!(
        s.library_files("cables").unwrap(),
        vec![
            ("a.ohlw".to_owned(), "a = 1".to_owned()),
            ("b.ohlw".to_owned(), "b = 1".to_owned())
        ]
    );
    let x0 = x(&s, "main", "T3");
    shift(&mut s, "ben", "T3", 5.0).unwrap();
    s.apply(
        "ben",
        Command::Delete {
            kind: Kind::Jumper,
            id: "J-T3-branch".into(),
            confirm: true,
        },
    )
    .unwrap();
    assert_eq!(rows(&path, "phase 1").1, 1, "a tombstone");
    assert!(s.line().jumpers.is_empty());
    assert_eq!(s.line_named("main").unwrap().jumpers.len(), 1);
    assert_eq!(x(&s, "main", "T3"), x0);
    let mut s = reopens_alike(&path, s);
    assert!(s.line().jumpers.is_empty(), "still deleted");
    // Undone: the jumper back, no tombstone.
    s.undo("ben").unwrap();
    assert_eq!(rows(&path, "phase 1").1, 0);
    let mut s = reopens_alike(&path, s);
    assert_eq!(s.line().jumpers.len(), 1);
    s.redo("ben").unwrap();
    // Main edited: the phase does not follow.
    s.select_line("main").unwrap();
    shift(&mut s, "cara", "T5", -2.0).unwrap();
    s.put_library_file("ana", "cables", "a.ohlw", "a = 2")
        .unwrap();
    assert_eq!(x(&s, "phase 1", "T3"), x0 + 5.0);
    let mut s = reopens_alike(&path, s);
    s.select_line("phase 1").unwrap();
    assert_eq!(s.library_files("cables").unwrap()[0].1, "a = 1");
    // A line from the phase (its uncommitted work recorded first); the
    // phase renamed, then deleted: the line made from it keeps its state.
    s.create_line("cara", "phase 2", "phase 1").unwrap();
    assert_eq!(s.line_named("phase 2").unwrap(), s.line());
    s.select_line("main").unwrap();
    s.rename_line("ben", "phase 1", "phase 1b").unwrap();
    let mut s = reopens_alike(&path, s);
    let phase1 = s.line_named("phase 1b").unwrap().clone();
    s.delete_line("ben", "phase 1b").unwrap();
    assert_eq!(rows(&path, "phase 1b"), (0, 0));
    let mut s = reopens_alike(&path, s);
    assert_eq!(s.line_named("phase 2").unwrap(), &phase1);
    // Main's changes brought in (a merge), then rolled back.
    s.select_line("phase 2").unwrap();
    s.bring_in("cara", "main", &Choices::new()).unwrap();
    assert_eq!(x(&s, "phase 2", "T5"), x(&s, "main", "T5"));
    assert_eq!(s.library_files("cables").unwrap()[0].1, "a = 2");
    let mut s = reopens_alike(&path, s);
    s.roll_back_line("ana", "phase 2", &start).unwrap();
    assert_eq!(x(&s, "phase 2", "T3"), x0);
    let mut s = reopens_alike(&path, s);
    // Carried as a bundle into a line of its own.
    let bundle = s.export_bundle("phase 2").unwrap();
    s.select_line("main").unwrap();
    s.import_bundle("cara", &bundle, Some("carried")).unwrap();
    assert_eq!(
        crate::lines::line_base(&s.conn, "carried").unwrap(),
        Some(bundle.head.clone())
    );
    let s = reopens_alike(&path, s);
    assert_eq!(x(&s, "carried", "T3"), x0);
}

/// A line whose state differs from its last revision only by the sign of
/// a zero (equal as values, not as text: nothing to commit) starts from
/// that revision with the differing object as a row of its own.
#[test]
fn a_signed_zero_is_kept() {
    let (path, mut s) = project("delta-zero");
    let p = s.line().structure("T2").unwrap().position;
    let mv = |y: f64| Command::MoveStructure {
        id: "T2".into(),
        position: [p[0], y, p[2]],
    };
    s.apply("ana", mv(0.0)).unwrap();
    s.commit("ana", "at zero").unwrap();
    s.apply("ana", mv(-0.0)).unwrap();
    assert!(s.status().unwrap().changes.is_empty(), "nothing to commit");
    assert!(s.has_uncommitted("main").unwrap(), "though not its text");
    s.create_line("ben", "phase 1", "main").unwrap();
    assert_eq!(rows(&path, "phase 1"), (1, 0), "T2 with -0.0");
    let s = reopens_alike(&path, s);
    let y = s
        .line_named("phase 1")
        .unwrap()
        .structure("T2")
        .unwrap()
        .position[1];
    assert!(y == 0.0 && y.is_sign_negative());
}

/// A line made from a revision is that revision's line with its library
/// files (owner, 2026-10-23: they started without them before), and keeps
/// nothing else; its library changes are its own rows.
#[test]
fn a_line_from_a_revision_keeps_nothing_else() {
    let (path, mut s) = project("delta-rev");
    s.put_library_file("ana", "cables", "a.ohlw", "a = 1")
        .unwrap();
    let rev = s.commit("ana", "first").unwrap();
    shift(&mut s, "ana", "T2", 1.0).unwrap();
    s.put_library_file("ana", "cables", "a.ohlw", "a = 2")
        .unwrap();
    s.create_line("ben", "old", &rev.id).unwrap();
    assert_eq!(rows(&path, "old"), (0, 0));
    assert_eq!(
        s.line_named("old").unwrap(),
        &s.revision_state(&rev.id).unwrap().line().unwrap()
    );
    // Its library files: the revision's (main has changed its own since).
    assert_eq!(
        s.line_library_files("old", "cables").unwrap(),
        [("a.ohlw".to_owned(), "a = 1".to_owned())]
    );
    assert_eq!(
        s.library_files("cables").unwrap(),
        [("a.ohlw".to_owned(), "a = 2".to_owned())]
    );
    let mut s = reopens_alike(&path, s);
    s.select_line("old").unwrap();
    assert_eq!(
        s.library_files("cables").unwrap(),
        [("a.ohlw".to_owned(), "a = 1".to_owned())]
    );
    // A library change on it: a row of its own.
    s.put_library_file("ben", "cables", "a.ohlw", "a = 3")
        .unwrap();
    assert_eq!(rows(&path, "old"), (1, 0));
    reopens_alike(&path, s);
}

/// A project of schema 5 with a line kept as a full copy opens and
/// works: its lines read as they are (nothing rewritten, versions kept),
/// an object deleted from a full copy leaves no tombstone, and a line made
/// from one keeps only its differences.
#[test]
fn full_copy_lines_of_older_files_open_and_work() {
    let (path, s) = project("full-copies");
    let main = s.line().clone();
    drop(s);
    {
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute_batch(
            "ALTER TABLE model_object DROP COLUMN gone;
             ALTER TABLE vc_ref DROP COLUMN base;
             INSERT INTO model_object (line, kind, id, ord, version, json)
                 SELECT 'copy', kind, id, ord, version + 2, json FROM model_object WHERE line = 'main';
             INSERT INTO vc_ref (name, kind, revision, user, note) VALUES ('copy', 'branch', NULL, 'ben', 'from main');
             UPDATE ols_meta SET value = '5' WHERE key = 'session_schema';",
        )
        .unwrap();
    }
    let before = rows_of(&path, "model_object", "copy");
    let mut s = Session::open(&path).unwrap();
    assert_eq!(s.line_named("copy").unwrap(), &main);
    assert_eq!(
        rows_of(&path, "model_object", "copy"),
        before,
        "nothing rewritten"
    );
    assert_eq!(crate::lines::line_base(&s.conn, "copy").unwrap(), None);
    assert_eq!(
        s.others["copy"].objects[&(Kind::Structure, "T2".to_owned())].version,
        3,
        "versions as stored"
    );
    s.select_line("copy").unwrap();
    s.apply(
        "ben",
        Command::Delete {
            kind: Kind::Jumper,
            id: "J-T3-branch".into(),
            confirm: true,
        },
    )
    .unwrap();
    assert_eq!(rows(&path, "copy"), (before - 1, 0), "no tombstone");
    s.create_line("ben", "from the copy", "copy").unwrap();
    assert_eq!(rows(&path, "from the copy"), (0, 0));
    let s = reopens_alike(&path, s);
    assert!(s.line_named("from the copy").unwrap().jumpers.is_empty());
    let schema: String = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT value FROM ols_meta WHERE key = 'session_schema'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(schema, SCHEMA_VERSION.to_string());
}
