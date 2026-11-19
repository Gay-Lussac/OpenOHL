// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The tests every [`Store`] must pass (ADR-0013, phase 4): the same
//! tests run on the project file ([`super::sqlite::SqliteStore`], this
//! crate's tests) and on a country server's database (`ohl-store-pg`'s
//! tests, with this crate's feature `store-suite`), so that the two modes
//! cannot drift apart.
//!
//! A store under test comes from an [`Open`] function: `open(name)` gives
//! the store of project `name` — empty the first time, the same project
//! again on the next call with that name (a file of that name, a model of
//! that code). Each test uses names of its own.
//!
//! [`run_all`] runs every test and returns the failures by name;
//! [`session_transcript`] runs one session through a store (import, edits,
//! undo, redo, a commit, a branch, a tag, the session opened again) and
//! returns what it did in words, without seq numbers or times — the same
//! on every store.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use ohl_core::grid::{Bounds, Grid};

use super::copy::{copy_all, lines_of};
use super::{
    EntryRow, GrantRow, NewEntry, NewSurface, RefRow, ReviewNoteRow, ReviewRow, RevisionRow, Store,
    SurfaceRow,
};
use crate::spatial::Entry;
use crate::{Command, Kind, Role, SCHEMA_VERSION, Session, Step};

/// `open(name)`: the store of project `name` (empty the first time).
pub type Open<'a> = &'a dyn Fn(&str) -> Box<dyn Store>;

/// A test of the suite.
pub type Test = fn(Open<'_>);

/// Every test, by name.
pub const TESTS: &[(&str, Test)] = &[
    ("prepare", prepare),
    ("meta_and_users", meta_and_users),
    ("objects", objects),
    ("units_of_work", units_of_work),
    ("log", log),
    ("spatial", spatial),
    ("versions", versions),
    ("lines", lines),
    ("collaboration", collaboration),
    ("surfaces", surfaces),
    ("whole_copies", whole_copies),
    ("session", session),
];

/// Runs every test of the suite on the stores `open` gives; returns the
/// failed tests with their messages (empty: all passed).
pub fn run_all(open: Open<'_>) -> Vec<(String, String)> {
    let mut failed = Vec::new();
    for (name, test) in TESTS {
        if let Err(e) = catch_unwind(AssertUnwindSafe(|| test(open))) {
            let message = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_default();
            failed.push(((*name).to_owned(), message));
        }
    }
    failed
}

fn prepared(open: Open<'_>, name: &str) -> Box<dyn Store> {
    let s = open(name);
    s.prepare(Path::new(name), true).expect("prepare");
    s
}

/// A time as the stores write them (`YYYY-MM-DDTHH:MM:SS.sssZ`).
fn is_time_ms(t: &str) -> bool {
    t.len() == 24 && t.ends_with('Z') && t.as_bytes()[10] == b'T' && t.as_bytes()[19] == b'.'
}

fn prepare(open: Open<'_>) {
    let s = open("suite-prepare");
    let grid = s.prepare(Path::new("suite-prepare"), true).unwrap();
    // Again: nothing changes.
    assert_eq!(s.prepare(Path::new("suite-prepare"), true).unwrap(), grid);
    assert_eq!(
        s.meta("session_schema").unwrap(),
        Some(SCHEMA_VERSION.to_string())
    );
    assert_eq!(
        s.meta("grid_size").unwrap(),
        Some(grid.size.to_string()),
        "the grid is kept"
    );
    assert!(s.versions_initialised(crate::versions::MAIN).unwrap());
    assert_eq!(s.meta("vc_head").unwrap().as_deref(), Some("branch:main"));
    let now = s.now().unwrap();
    assert!(now.len() == 20 && now.ends_with('Z'), "{now}");
    s.mark_object_upgrades_done().unwrap();
    assert_eq!(s.meta("structure_groups").unwrap().as_deref(), Some("1"));
}

fn meta_and_users(open: Open<'_>) {
    let s = prepared(open, "suite-meta");
    assert_eq!(s.meta("setting.x").unwrap(), None);
    s.set_meta("setting.x", "1").unwrap();
    s.set_meta("setting.x", "2").unwrap();
    assert_eq!(s.meta("setting.x").unwrap().as_deref(), Some("2"));
    let rows = s.meta_rows().unwrap();
    assert!(rows.windows(2).all(|w| w[0].0 < w[1].0), "by key");
    assert!(rows.contains(&("setting.x".into(), "2".into())));
    s.delete_meta("setting.x").unwrap();
    assert_eq!(s.meta("setting.x").unwrap(), None);
    assert_eq!(s.user_count().unwrap(), 0);
    s.put_user("ben", "designer").unwrap();
    s.put_user("ana", "admin").unwrap();
    s.put_user("ben", "viewer").unwrap();
    assert_eq!(s.user_count().unwrap(), 2);
    assert_eq!(s.user_role("ben").unwrap().as_deref(), Some("viewer"));
    assert_eq!(s.user_role("cy").unwrap(), None);
    assert_eq!(
        s.users().unwrap(),
        vec![
            ("ana".to_owned(), "admin".to_owned()),
            ("ben".to_owned(), "viewer".to_owned())
        ]
    );
}

type RowKey = (String, String, String, i64, i64, String, i64);

fn rows_of(s: &dyn Store) -> Vec<RowKey> {
    let mut v: Vec<RowKey> = s
        .object_rows()
        .unwrap()
        .into_iter()
        .map(|r| (r.line, r.kind, r.id, r.ord, r.version, r.json, r.gone))
        .collect();
    v.sort();
    v
}

fn objects(open: Open<'_>) {
    let s = prepared(open, "suite-objects");
    let main = crate::versions::MAIN;
    s.write_object(
        main,
        Kind::Structure,
        "so-T1",
        Some((0, 1, r#"{"id":"so-T1"}"#)),
    )
    .unwrap();
    s.write_object(
        main,
        Kind::Structure,
        "so-T1",
        Some((3, 2, r#"{"id":"so-T1","x":1}"#)),
    )
    .unwrap();
    s.write_object("b", Kind::Structure, "so-T1", None).unwrap();
    s.write_place("b", Kind::Structure, "so-T2", 3, 4).unwrap();
    s.replace_object(main, Kind::Section, "so-S1", 1, 1, "{}")
        .unwrap();
    s.replace_object(main, Kind::Section, "so-S1", 2, 5, r#"{"a":[1,2.5]}"#)
        .unwrap();
    let t = |l: &str, k: &str, id: &str, o: i64, v: i64, j: &str, g: i64| -> RowKey {
        (l.into(), k.into(), id.into(), o, v, j.into(), g)
    };
    assert_eq!(
        rows_of(&*s),
        vec![
            t("b", "structure", "so-T1", 0, 0, "null", 1),
            t("b", "structure", "so-T2", 3, 4, "null", 2),
            t(main, "section", "so-S1", 2, 5, r#"{"a":[1,2.5]}"#, 0),
            t(
                main,
                "structure",
                "so-T1",
                3,
                2,
                r#"{"id":"so-T1","x":1}"#,
                0
            ),
        ]
    );
    // The JSON text is kept as it is (not re-encoded).
    let odd = r#"{ "b" : 1.0,  "a":"éé" }"#;
    s.write_object(main, Kind::Jumper, "so-J1", Some((0, 1, odd)))
        .unwrap();
    assert!(rows_of(&*s).iter().any(|r| r.5 == odd));
    s.delete_object(main, Kind::Section, "so-S1").unwrap();
    s.delete_object(main, Kind::Jumper, "so-J1").unwrap();
    assert_eq!(rows_of(&*s).len(), 3);
}

fn units_of_work(open: Open<'_>) {
    let s = prepared(open, "suite-units");
    let m = |k: &str| s.meta(k).unwrap();
    s.savepoint("a").unwrap();
    s.set_meta("u.x", "1").unwrap();
    s.savepoint("b").unwrap();
    s.set_meta("u.y", "2").unwrap();
    s.roll_back_to("b").unwrap();
    assert_eq!(m("u.y"), None);
    assert_eq!(m("u.x").as_deref(), Some("1"));
    s.savepoint("c").unwrap();
    s.set_meta("u.z", "3").unwrap();
    s.release("c").unwrap();
    s.release("a").unwrap();
    assert_eq!(m("u.x").as_deref(), Some("1"));
    assert_eq!(m("u.z").as_deref(), Some("3"));
    // The outermost step rolled back: nothing of it stays.
    s.savepoint("d").unwrap();
    s.set_meta("u.w", "4").unwrap();
    s.savepoint("e").unwrap();
    s.set_meta("u.v", "5").unwrap();
    s.release("e").unwrap();
    s.roll_back_to("d").unwrap();
    assert_eq!(m("u.w"), None);
    assert_eq!(m("u.v"), None);
    // A failed statement inside a step: the step rolled back, the store
    // usable after it.
    s.put_blob("u-blob", b"1").unwrap();
    {
        let tx = Step::begin(&*s).unwrap();
        tx.set_meta("u.f", "1").unwrap();
        assert!(tx.put_blob("u-blob", b"2").is_err(), "a blob twice");
        drop(tx);
    }
    assert_eq!(m("u.f"), None);
    assert_eq!(s.blob("u-blob").unwrap().as_deref(), Some(&b"1"[..]));
    // Steps through the session's helper, nested.
    {
        let outer = Step::begin(&*s).unwrap();
        outer.set_meta("u.o", "1").unwrap();
        let inner = Step::begin(&*s).unwrap();
        inner.set_meta("u.i", "1").unwrap();
        inner.commit().unwrap();
        outer.commit().unwrap();
    }
    assert_eq!(m("u.o").as_deref(), Some("1"));
    assert_eq!(m("u.i").as_deref(), Some("1"));
}

fn entry<'a>(user: &'a str, kind: &'a str, line: &'a str, n: u32) -> NewEntry<'a> {
    NewEntry {
        user,
        kind,
        command: format!(r#"{{"op":"batch","commands":[],"n":{n}}}"#),
        inverse: format!(r#"{{"inverse":{n}}}"#),
        objects: format!(r#"[["structure","T{n}",{n}]]"#),
        sections: format!(r#"["S{n}"]"#),
        undoes: None,
        note: None,
        line,
    }
}

fn log(open: Open<'_>) {
    let s = prepared(open, "suite-log");
    let main = crate::versions::MAIN;
    assert_eq!(s.last_seq().unwrap(), 0);
    assert!(s.entries().unwrap().is_empty());
    let mut first = entry("ana", "do", main, 1);
    first.note = Some("first".into());
    let e1 = s.append_entry(&first).unwrap();
    let e2 = s.append_entry(&entry("ben", "do", main, 2)).unwrap();
    let e3 = s.append_entry(&entry("ana", "do", "b", 3)).unwrap();
    assert!(e1 < e2 && e2 < e3);
    assert_eq!(s.last_undoable("ana", main).unwrap(), Some(e1));
    let mut undo = entry("ana", "undo", main, 4);
    undo.undoes = Some(e1);
    let e4 = s.append_entry(&undo).unwrap();
    s.mark_undone(e1, e4).unwrap();
    s.note_reversal(e4, e1).unwrap();
    assert_eq!(s.last_undoable("ana", main).unwrap(), None);
    assert_eq!(s.last_undoable("ana", "b").unwrap(), Some(e3));
    assert_eq!(
        s.last_own_entry("ana", main).unwrap(),
        Some((e4, "undo".to_owned(), None))
    );
    assert_eq!(s.last_own_entry("cy", main).unwrap(), None);
    let row: EntryRow = s.entry(e4).unwrap().unwrap();
    assert_eq!(row.0, e4);
    assert!(is_time_ms(&row.1), "{}", row.1);
    assert_eq!((row.2.as_str(), row.3.as_str()), ("ana", "undo"));
    assert_eq!(row.8, Some(e1));
    assert_eq!(row.10, Some(format!("reverses {e1}: first")));
    assert_eq!(row.11, main);
    // A reversal of an entry without a note.
    s.note_reversal(e2, e3).unwrap();
    assert_eq!(
        s.entry(e2).unwrap().unwrap().10,
        Some(format!("reverses {e3}"))
    );
    s.set_entry_note(e2, "plain").unwrap();
    assert_eq!(s.entry(e2).unwrap().unwrap().10.as_deref(), Some("plain"));
    assert_eq!(s.entry(e1).unwrap().unwrap().9, Some(e4));
    assert_eq!(
        s.entry_objects(e2).unwrap().as_deref(),
        Some(r#"[["structure","T2",2]]"#)
    );
    assert_eq!(s.entry(e4 + 1000).unwrap(), None);
    assert_eq!(
        s.inverses_after(e1, main).unwrap(),
        vec![r#"{"inverse":4}"#.to_owned(), r#"{"inverse":2}"#.to_owned()]
    );
    assert_eq!(s.last_seq().unwrap(), e4);
    assert_eq!(s.last_seq_of_line("b").unwrap(), e3);
    assert_eq!(s.last_seq_of_line("none").unwrap(), 0);
    // Everything after a point reversed by one entry.
    let e5 = s.append_entry(&entry("ben", "do", main, 5)).unwrap();
    let e6 = s.append_entry(&entry("ana", "do", main, 6)).unwrap();
    let e7 = s.append_entry(&entry("ana", "rollback", main, 7)).unwrap();
    s.mark_undone_after(e7, e4, main).unwrap();
    let all = s.entries().unwrap();
    assert_eq!(all.len(), 7);
    assert!(all.windows(2).all(|w| w[0].0 < w[1].0), "oldest first");
    let undone: BTreeMap<i64, Option<i64>> = all.iter().map(|r| (r.0, r.9)).collect();
    assert_eq!(undone[&e5], Some(e7));
    assert_eq!(undone[&e6], Some(e7));
    assert_eq!(undone[&e3], None, "another line");
    assert_eq!(undone[&e7], None);
}

fn spatial_entry(grid: &Grid, a: [f64; 2], b: [f64; 2]) -> Entry {
    let bounds = Bounds::new(a, b);
    Entry {
        bounds,
        tile: grid.home(&bounds),
    }
}

fn spatial(open: Open<'_>) {
    let s = prepared(open, "suite-spatial");
    let grid = Grid::default();
    let main = crate::versions::MAIN;
    let mut old = BTreeMap::new();
    old.insert(
        (Kind::Structure, "sp-T1".to_owned()),
        spatial_entry(&grid, [10.0, 10.0], [12.0, 14.0]),
    );
    old.insert(
        (Kind::Section, "sp-S1".to_owned()),
        spatial_entry(&grid, [10.0, 10.0], [900.0, 50.0]),
    );
    assert_eq!(
        s.spatial_write_diff(main, &BTreeMap::new(), &old).unwrap(),
        2
    );
    assert_eq!(s.spatial_load(main).unwrap(), old);
    assert!(s.spatial_load("b").unwrap().is_empty());
    let mut new = old.clone();
    new.remove(&(Kind::Section, "sp-S1".to_owned()));
    new.insert(
        (Kind::Structure, "sp-T1".to_owned()),
        spatial_entry(&grid, [5000.0, 5000.0], [5001.0, 5003.5]),
    );
    new.insert(
        (Kind::Jumper, "sp-J1".to_owned()),
        spatial_entry(&grid, [-20.0, -30.0], [-10.0, -29.0]),
    );
    // One removed, one changed, one added.
    assert_eq!(s.spatial_write_diff(main, &old, &new).unwrap(), 3);
    assert_eq!(s.spatial_load(main).unwrap(), new);
    let hits = s
        .spatial_query(
            main,
            &grid,
            &Bounds::new([4990.0, 4990.0], [5010.0, 5010.0]),
        )
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].0, hits[0].1.as_str()), (Kind::Structure, "sp-T1"));
    let hits = s
        .spatial_query(
            main,
            &grid,
            &Bounds::new([-100.0, -100.0], [6000.0, 6000.0]),
        )
        .unwrap();
    let ids: Vec<&str> = hits.iter().map(|h| h.1.as_str()).collect();
    assert_eq!(ids, vec!["sp-T1", "sp-J1"], "by kind, then id");
    assert!(
        s.spatial_query("b", &grid, &Bounds::new([0.0, 0.0], [1e6, 1e6]))
            .unwrap()
            .is_empty()
    );
}

fn revision(id: &str, parents: &str, seq: i64) -> RevisionRow {
    RevisionRow {
        id: id.into(),
        parents: parents.into(),
        author: "ana".into(),
        time: String::new(),
        message: format!("message {id}"),
        tree: format!("tree-{id}"),
        seq,
    }
}

fn versions(open: Open<'_>) {
    let s = prepared(open, "suite-versions");
    assert!(!s.has_blob("h1").unwrap());
    s.put_blob("h1", &[0, 1, 2, 255]).unwrap();
    assert!(s.has_blob("h1").unwrap());
    assert_eq!(s.blob("h1").unwrap(), Some(vec![0, 1, 2, 255]));
    assert_eq!(s.blob("h2").unwrap(), None);
    s.put_revision(&revision("rev-aa1", "[]", 1)).unwrap();
    s.put_revision(&revision("rev-ab2", r#"["rev-aa1"]"#, 2))
        .unwrap();
    let mut kept = revision("rev-b3", r#"["rev-ab2"]"#, 3);
    kept.time = "2026-01-02T03:04:05.678Z".into();
    s.put_revision_with_time(&kept).unwrap();
    let all = s.revisions().unwrap();
    let ids: Vec<&str> = all.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["rev-b3", "rev-ab2", "rev-aa1"], "newest first");
    assert_eq!(s.revision("rev-b3").unwrap(), Some(kept));
    let r1 = s.revision("rev-aa1").unwrap().unwrap();
    assert!(is_time_ms(&r1.time), "{}", r1.time);
    assert_eq!(
        (r1.parents.as_str(), r1.tree.as_str(), r1.seq),
        ("[]", "tree-rev-aa1", 1)
    );
    assert_eq!(s.revisions_starting("rev-a").unwrap().len(), 2);
    assert_eq!(s.revisions_starting("rev-b").unwrap(), vec!["rev-b3"]);
    assert!(s.has_revision("rev-ab2").unwrap());
    assert!(!s.has_revision("rev-zz").unwrap());
    // Branches and tags.
    s.add_branch("b1", "ana").unwrap();
    s.add_branch_noted("b2", "ben", "kept").unwrap();
    s.add_line_branch("b3", Some("rev-aa1"), "ana", "a phase")
        .unwrap();
    s.add_tag("t1", "rev-aa1", "ana", "issued").unwrap();
    assert_eq!(s.line_base("b3").unwrap().as_deref(), Some("rev-aa1"));
    assert_eq!(s.line_base("b1").unwrap(), None);
    assert_eq!(s.line_base("t1").unwrap(), None, "a tag has no base");
    s.set_branch_revision("b1", "rev-ab2").unwrap();
    s.set_branch_revision("t1", "rev-ab2").unwrap();
    s.set_ref_revision("t1", "rev-b3").unwrap();
    s.set_protected("b1", true).unwrap();
    s.set_line_description("b1", "described").unwrap();
    s.set_line_base("b1", "rev-aa1").unwrap();
    assert_eq!(
        s.line_description("b1").unwrap().as_deref(),
        Some("described")
    );
    assert_eq!(s.line_description("t1").unwrap(), None);
    assert_eq!(s.line_base("b1").unwrap().as_deref(), Some("rev-aa1"));
    let refs = s.refs().unwrap();
    let names: Vec<(&str, &str)> = refs
        .iter()
        .map(|r| (r.kind.as_str(), r.name.as_str()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("branch", "b1"),
            ("branch", "b2"),
            ("branch", "b3"),
            ("branch", "main"),
            ("tag", "t1")
        ]
    );
    let b1 = &refs[0];
    assert_eq!(
        (b1.revision.as_deref(), b1.protected, b1.user.as_str()),
        (Some("rev-ab2"), true, "ana")
    );
    assert!(is_time_ms(&b1.time), "{}", b1.time);
    assert_eq!(refs[1].note, "kept");
    assert_eq!(refs[4].revision.as_deref(), Some("rev-b3"));
    s.delete_ref("b2").unwrap();
    assert_eq!(s.refs().unwrap().len(), 4);
    // Made again: nothing changes.
    s.init_versions("main").unwrap();
    assert_eq!(s.refs().unwrap().len(), 4);
}

fn lines(open: Open<'_>) {
    let s = prepared(open, "suite-lines");
    let grid = Grid::default();
    s.write_object("p1", Kind::Structure, "ln-T1", Some((0, 1, "{}")))
        .unwrap();
    s.write_object("main", Kind::Structure, "ln-T1", Some((0, 1, "{}")))
        .unwrap();
    let e = s.append_entry(&entry("ana", "do", "p1", 1)).unwrap();
    let mut index = BTreeMap::new();
    index.insert(
        (Kind::Structure, "ln-T1".to_owned()),
        spatial_entry(&grid, [0.0, 0.0], [1.0, 1.0]),
    );
    s.spatial_write_diff("p1", &BTreeMap::new(), &index)
        .unwrap();
    s.add_branch("p1", "ana").unwrap();
    let review = s
        .add_review(&ReviewRow {
            id: 0,
            branch: "p1".into(),
            target: "main".into(),
            revision: "r".into(),
            author: "ana".into(),
            time: String::new(),
            note: String::new(),
            reviewers: "[]".into(),
            state: String::new(),
        })
        .unwrap();
    s.rename_line("p1", "p2").unwrap();
    let lines_now: Vec<String> = lines_of(&*s).unwrap().into_iter().collect();
    assert_eq!(lines_now, vec!["main".to_owned(), "p2".to_owned()]);
    assert_eq!(s.entry(e).unwrap().unwrap().11, "p2");
    assert_eq!(s.spatial_load("p2").unwrap(), index);
    assert!(s.spatial_load("p1").unwrap().is_empty());
    assert_eq!(s.open_review("p2", "main").unwrap(), Some(review));
    s.delete_line("p2").unwrap();
    assert!(s.object_rows().unwrap().iter().all(|r| r.line == "main"));
    assert!(s.spatial_load("p2").unwrap().is_empty());
    assert!(s.refs().unwrap().iter().all(|r| r.name != "p2"));
    assert_eq!(s.entries().unwrap().len(), 1, "the log stays");
    assert_eq!(s.legacy_snapshots().unwrap(), None);
}

fn collaboration(open: Open<'_>) {
    let s = prepared(open, "suite-collab");
    let grant = |subject: &str| GrantRow {
        id: 0,
        subject: subject.into(),
        kinds: r#"["structure"]"#.into(),
        objects: "[]".into(),
        area: Some("[[0,0],[1,0],[1,1]]".into()),
        user: "ana".into(),
        time: String::new(),
        note: "n".into(),
    };
    let g1 = s.add_grant(&grant("ben")).unwrap();
    let g2 = s.add_grant(&grant("designer")).unwrap();
    assert!(g1 < g2);
    let grants = s.grants().unwrap();
    assert_eq!(grants.len(), 2);
    assert_eq!(grants[0].id, g1);
    assert_eq!(grants[0].area.as_deref(), Some("[[0,0],[1,0],[1,1]]"));
    assert!(is_time_ms(&grants[0].time), "{}", grants[0].time);
    s.delete_grant(g1).unwrap();
    assert_eq!(s.grants().unwrap().len(), 1);
    let review = |branch: &str| ReviewRow {
        id: 0,
        branch: branch.into(),
        target: "main".into(),
        revision: "rev".into(),
        author: "ben".into(),
        time: String::new(),
        note: "please".into(),
        reviewers: r#"["ana"]"#.into(),
        state: String::new(),
    };
    let r1 = s.add_review(&review("b1")).unwrap();
    let r2 = s.add_review(&review("b2")).unwrap();
    assert_eq!(s.open_review("b1", "main").unwrap(), Some(r1));
    s.set_review_state(r1, "approved").unwrap();
    assert_eq!(s.open_review("b1", "main").unwrap(), Some(r1));
    s.set_review_state(r1, "merged").unwrap();
    assert_eq!(s.open_review("b1", "main").unwrap(), None);
    let reviews = s.reviews().unwrap();
    assert_eq!(
        reviews.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![r2, r1]
    );
    assert_eq!(reviews[1].state, "merged");
    assert_eq!(reviews[0].state, "open");
    s.add_review_note(r1, "ana", "approve", "fine").unwrap();
    s.add_review_note(r2, "ana", "reject", "no").unwrap();
    let notes = s.review_notes().unwrap();
    assert_eq!(notes.len(), 2);
    assert_eq!(
        (notes[0].review, notes[0].verdict.as_str()),
        (r1, "approve")
    );
    assert!(is_time_ms(&notes[1].time));
}

fn surfaces(open: Open<'_>) {
    let s = prepared(open, "suite-surfaces");
    let surface = |name: &'static str, data: Vec<u8>| NewSurface {
        name,
        user: "ana",
        source: "survey.csv",
        points: 3,
        triangles: 1,
        bounds: "[0,0,1,1]".into(),
        data,
    };
    s.put_surface(&surface("ground", vec![1, 2, 3])).unwrap();
    s.put_surface(&surface("a-first", vec![9])).unwrap();
    s.put_surface(&surface("ground", vec![4, 5])).unwrap();
    let all = s.surfaces().unwrap();
    assert_eq!(
        all.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        vec!["a-first", "ground"]
    );
    assert!(is_time_ms(&all[1].time));
    assert_eq!(s.surface_data("ground").unwrap(), Some(vec![4, 5]));
    assert_eq!(s.rename_surface("ground", "terrain").unwrap(), 1);
    assert_eq!(s.rename_surface("ground", "x").unwrap(), 0);
    assert_eq!(s.surface_data("terrain").unwrap(), Some(vec![4, 5]));
    assert_eq!(s.delete_surface("terrain").unwrap(), 1);
    assert_eq!(s.delete_surface("terrain").unwrap(), 0);
    assert_eq!(s.surfaces().unwrap().len(), 1);
}

/// Everything a store keeps that the interface reads, in words (ids of
/// grants and reviews, seq numbers and times included).
pub fn snapshot(s: &dyn Store) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "meta {:?}", s.meta_rows().unwrap());
    let _ = writeln!(out, "users {:?}", s.users().unwrap());
    for r in rows_of(s) {
        let _ = writeln!(out, "object {r:?}");
    }
    for e in s.entries().unwrap() {
        let _ = writeln!(out, "entry {e:?}");
    }
    for line in lines_of(s).unwrap() {
        let _ = writeln!(out, "spatial {line} {:?}", s.spatial_load(&line).unwrap());
    }
    for h in s.blob_hashes().unwrap() {
        let _ = writeln!(out, "blob {h} {:?}", s.blob(&h).unwrap());
    }
    for r in s.revisions().unwrap() {
        let _ = writeln!(out, "revision {r:?}");
    }
    for r in s.refs().unwrap() {
        let _ = writeln!(
            out,
            "ref {r:?} {:?} {:?}",
            s.line_description(&r.name).unwrap(),
            s.line_base(&r.name).unwrap()
        );
    }
    for g in s.grants().unwrap() {
        let _ = writeln!(out, "grant {g:?}");
    }
    for r in s.reviews().unwrap() {
        let _ = writeln!(out, "review {r:?}");
    }
    for n in s.review_notes().unwrap() {
        let _ = writeln!(out, "note {n:?}");
    }
    for r in s.surfaces().unwrap() {
        let _ = writeln!(out, "surface {r:?} {:?}", s.surface_data(&r.name).unwrap());
    }
    out
}

fn whole_copies(open: Open<'_>) {
    let s = prepared(open, "suite-copy-from");
    let grid = Grid::default();
    s.set_meta("setting.copy", "yes").unwrap();
    s.put_user("ana", "admin").unwrap();
    s.write_object(
        "main",
        Kind::Structure,
        "cp-T1",
        Some((2, 3, r#"{"id":"cp-T1"}"#)),
    )
    .unwrap();
    s.write_object("b", Kind::Structure, "cp-T1", None).unwrap();
    s.write_place("b", Kind::Structure, "cp-T2", 1, 1).unwrap();
    // Rows kept as they are: seq, time, marks.
    let row: EntryRow = (
        41,
        "2025-12-31T23:59:59.999Z".into(),
        "ana".into(),
        "do".into(),
        r#"{"op":"batch","commands":[]}"#.into(),
        "{}".into(),
        "[]".into(),
        "[]".into(),
        None,
        Some(42),
        Some("kept".into()),
        "main".into(),
    );
    s.put_entry_row(&row).unwrap();
    assert_eq!(s.entry(41).unwrap(), Some(row));
    let e = s.append_entry(&entry("ana", "undo", "main", 2)).unwrap();
    assert!(e > 41, "after the entry kept");
    let mut index = BTreeMap::new();
    index.insert(
        (Kind::Structure, "cp-T1".to_owned()),
        spatial_entry(&grid, [3.0, 4.0], [5.0, 6.0]),
    );
    s.spatial_write_diff("main", &BTreeMap::new(), &index)
        .unwrap();
    s.put_blob("cp-h", b"blob").unwrap();
    let mut rev = revision("cp-r1", "[]", 41);
    rev.time = "2025-12-31T23:59:59.000Z".into();
    s.put_revision_with_time(&rev).unwrap();
    s.put_revision(&revision("cp-r2", r#"["cp-r1"]"#, 43))
        .unwrap();
    let branch = RefRow {
        name: "phase".into(),
        kind: "branch".into(),
        revision: Some("cp-r1".into()),
        protected: true,
        user: "ana".into(),
        time: "2025-06-01T00:00:00.000Z".into(),
        note: "n".into(),
    };
    s.put_ref_row(&branch, "described", Some("cp-r1")).unwrap();
    assert!(s.refs().unwrap().contains(&branch));
    assert_eq!(s.line_base("phase").unwrap().as_deref(), Some("cp-r1"));
    let grant = GrantRow {
        id: 7,
        subject: "ben".into(),
        kinds: "[]".into(),
        objects: r#"["cp-T1"]"#.into(),
        area: None,
        user: "ana".into(),
        time: "2025-06-01T00:00:01.000Z".into(),
        note: String::new(),
    };
    s.put_grant_row(&grant).unwrap();
    assert_eq!(s.grants().unwrap(), vec![grant]);
    let review = ReviewRow {
        id: 5,
        branch: "phase".into(),
        target: "main".into(),
        revision: "cp-r1".into(),
        author: "ben".into(),
        time: "2025-06-01T00:00:02.000Z".into(),
        note: "x".into(),
        reviewers: "[]".into(),
        state: "rejected".into(),
    };
    s.put_review_row(&review).unwrap();
    assert_eq!(s.reviews().unwrap(), vec![review]);
    let note = ReviewNoteRow {
        review: 5,
        user: "ana".into(),
        time: "2025-06-01T00:00:03.000Z".into(),
        verdict: "reject".into(),
        comment: "c".into(),
    };
    s.put_review_note_row(&note).unwrap();
    assert_eq!(s.review_notes().unwrap(), vec![note]);
    let surface = SurfaceRow {
        name: "ground".into(),
        time: "2025-06-01T00:00:04.000Z".into(),
        user: "ana".into(),
        source: "s".into(),
        points: 3,
        triangles: 1,
        bounds: "[]".into(),
    };
    s.put_surface_row(&surface, &[7, 7]).unwrap();
    assert_eq!(s.surfaces().unwrap(), vec![surface]);
    // The whole store into another one: the same, read through the
    // interface.
    let to = open("suite-copy-to");
    let (_, copied) = copy_all(&*s, &*to, Path::new("suite-copy-to")).unwrap();
    assert_eq!(copied.objects, 3);
    assert_eq!(copied.entries, 2);
    assert_eq!(copied.revisions, 2);
    assert_eq!(snapshot(&*s), snapshot(&*to));
}

/// The example line of the repository (invented).
fn example() -> ohl_line::Line {
    ohl_line::Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .expect("the example line")
}

fn session(open: Open<'_>) {
    let a = session_transcript(open, "suite-session-a");
    let b = session_transcript(open, "suite-session-b");
    assert_eq!(a, b, "the same work gives the same transcript");
}

/// Runs one session through the store `open(name)` gives (an empty
/// project): the example line imported, a second user, a structure moved,
/// undone and redone, a commit, a branch, a tag, an edit on the branch
/// undone, the session opened again. Returns what it holds afterwards in
/// words, with log entries by their place (not their seq numbers) and no
/// times — the same on every store.
pub fn session_transcript(open: Open<'_>, name: &str) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    {
        let mut s = Session::open_on(open(name), name).expect("open");
        s.import(&example(), "ana").expect("import");
        s.set_user("ana", "ben", Role::Designer).unwrap();
        let p = s.line().structure("T6").unwrap().position;
        let moved = Command::MoveStructure {
            id: "T6".into(),
            position: [p[0] + 5.0, p[1], p[2]],
        };
        let applied = s.apply("ben", moved).unwrap();
        let _ = writeln!(out, "moved: sections {:?}", applied.sections);
        s.undo("ben").unwrap();
        s.redo("ben").unwrap();
        let rev = s.commit("ana", "first").unwrap();
        let _ = writeln!(out, "commit: {} {}", rev.message, rev.author);
        s.create_branch("ana", "phase 1", None).unwrap();
        s.tag("ana", "issued", None, "for the record").unwrap();
        let _ = writeln!(out, "line worked on: {}", s.current_line());
        s.put_library_file("ana", "weather", "cases", "[cases]\n")
            .unwrap();
        let _ = writeln!(
            out,
            "library: {:?}",
            s.library_files("weather").unwrap().len()
        );
    }
    let s = Session::open_on(open(name), name).expect("open again");
    let line = s.export();
    let _ = writeln!(
        out,
        "line: {}",
        serde_json::to_string(&line).unwrap_or_default()
    );
    let log = s.log().unwrap();
    let place: BTreeMap<i64, usize> = log.iter().enumerate().map(|(k, e)| (e.seq, k)).collect();
    for e in &log {
        let _ = writeln!(
            out,
            "entry {} {} {} {} objects {:?} sections {:?} undoes {:?} undone by {:?} note {:?}",
            place[&e.seq],
            e.user,
            e.kind,
            e.line,
            e.objects,
            e.sections,
            e.undoes.map(|q| place[&q]),
            e.undone_by.map(|q| place[&q]),
            e.note
                .as_deref()
                .map(|n| n.split(':').next().unwrap_or_default().split(' ').next())
        );
    }
    for r in s.refs().unwrap() {
        let _ = writeln!(out, "ref {} {:?}", r.name, r.kind);
    }
    let _ = writeln!(out, "revisions {}", s.revisions().unwrap().len());
    let _ = writeln!(out, "users {:?}", s.users().unwrap());
    out
}
