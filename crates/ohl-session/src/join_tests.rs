// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The join of another project's routes ([`crate::join`]) on the
//! synthetic example line split in two project files at its Y tower T3:
//! the main route in the open project, the branch (with its own copy of
//! T3) in the other file.

use std::path::{Path, PathBuf};

use ohl_line::Line;

use crate::join::{self, JoinOptions, Outcome, TwinState};
use crate::{Role, Session};

fn example() -> Line {
    Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap()
}

/// The main route alone (T1–T5, S1, S2).
fn main_route() -> Line {
    let mut l = example();
    l.routes.retain(|r| r.id == "main");
    l.alignments.retain(|a| a.id == "main");
    l.alignment_points
        .retain(|p| ["P1", "P2", "P3"].contains(&p.id.as_str()));
    l.structures
        .retain(|s| !["T6", "T7"].contains(&s.id.as_str()));
    l.sections.retain(|s| s.id != "S3");
    l.jumpers.clear();
    l.circuits.retain(|c| c.name != "Branch");
    l
}

/// The branch alone, with its own T3 (T3, T6, T7, S3).
fn branch() -> Line {
    let mut l = example();
    l.name = "Branch line".into();
    l.routes.retain(|r| r.id == "branch");
    l.alignments.retain(|a| a.id == "branch");
    l.alignment_points
        .retain(|p| ["P2", "P4", "P5"].contains(&p.id.as_str()));
    l.structures
        .retain(|s| ["T3", "T6", "T7"].contains(&s.id.as_str()));
    l.sections.retain(|s| s.id == "S3");
    l.jumpers.clear();
    l.circuits.retain(|c| c.name == "Branch");
    l
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ohl-join-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn project(path: &Path, line: &Line) -> Session {
    let mut s = Session::open(path).unwrap();
    s.import(line, "ana").unwrap();
    s.set_user("ana", "ben", Role::Designer).unwrap();
    s
}

fn libs(s: &Session) -> Vec<(String, String, String)> {
    s.line_library_all(s.current_line()).unwrap()
}

fn plan(open: &Session, other: &Path, options: &JoinOptions) -> join::JoinPlan {
    join::plan(&open.export(), &libs(open), other, options).unwrap()
}

#[test]
fn the_shared_helpers_are_the_network_imports() {
    let taken = ["B-T1".to_owned(), "B-T1-2".to_owned()];
    let id = join::fresh_id("B", "T1", |c| -> Result<bool, ()> {
        Ok(taken.iter().any(|t| t == c))
    })
    .unwrap();
    assert_eq!(id, "B-T1-3");
    assert_eq!(
        join::twin_distance([0.0, 0.0], Some("a"), [0.3, 0.4], Some("a"), 0.5),
        Some(0.5)
    );
    assert_eq!(
        join::twin_distance([0.0, 0.0], Some("a"), [0.3, 0.4], Some("b"), 0.5),
        None,
        "another type"
    );
    assert_eq!(
        join::twin_distance([0.0, 0.0], None, [0.3, 0.41], None, 0.5),
        None,
        "too far"
    );
    assert!(join::check_prefix("B-2").is_ok());
    assert!(join::check_prefix("-B").is_err());
    assert!(join::check_prefix("").is_err());
}

/// Routes joined, ids clashing renamed, the twin unticked: two towers.
#[test]
fn routes_joined_ids_renamed_twin_unticked_stays_two_towers() {
    let d = dir("unticked");
    let mut open = project(&d.join("main.ohl"), &main_route());
    drop(project(&d.join("branch.ohl"), &branch()));
    let other = d.join("branch.ohl");
    let bytes = std::fs::read(&other).unwrap();
    let p = plan(&open, &other, &JoinOptions::new("B"));
    assert!(p.can_apply(), "{:?}", p.blocked);
    assert_eq!(p.routes, vec![("branch".to_owned(), "branch".to_owned())]);
    let renamed: Vec<(String, String, String)> = p
        .renamed
        .iter()
        .map(|r| (r.kind.clone(), r.from.clone(), r.to.clone()))
        .collect();
    assert!(renamed.contains(&("structure".into(), "T3".into(), "B-T3".into())));
    assert!(renamed.contains(&("alignment_point".into(), "P2".into(), "B-P2".into())));
    assert!(renamed.contains(&("system".into(), "ex220".into(), "B-ex220".into())));
    // The twin listed, not ticked.
    assert_eq!(p.twins.len(), 1);
    let t = &p.twins[0];
    assert_eq!((t.open.as_str(), t.other.as_str()), ("T3", "T3"));
    assert_eq!(t.joined, "B-T3");
    assert_eq!(t.distance_m, 0.0);
    assert_eq!(t.height_difference_m, Some(0.0));
    assert_eq!(t.orientation_difference_deg, 0.0);
    assert!(!t.ticked);
    assert_eq!(t.state, TwinState::Apart);
    // Applied: one command, one revision.
    let log_before = open.log().unwrap().len();
    let (_, rev) = join::join(&mut open, "ben", &p).unwrap();
    assert_eq!(open.log().unwrap().len(), log_before + 1, "one log entry");
    assert_eq!(rev.message, "joined routes from branch.ohl");
    let l = open.export();
    assert_eq!(l.structures.len(), 8, "T3 and B-T3: two towers");
    let s3 = l.sections.iter().find(|s| s.id == "S3").unwrap();
    assert_eq!(s3.points[0].structure, "B-T3");
    assert_eq!(s3.points[0].set, 2);
    let br = l.alignments.iter().find(|a| a.id == "branch").unwrap();
    assert_eq!(br.points[0], "B-P2");
    assert_eq!(
        l.alignment_point("B-P2").unwrap().structure.as_deref(),
        Some("B-T3")
    );
    // The other file is read, never changed.
    assert_eq!(std::fs::read(&other).unwrap(), bytes);
}

/// A ticked twin: one shared tower, the other line's section re-attached
/// by label; undone at once, exactly.
#[test]
fn ticked_twin_becomes_one_shared_tower_and_undo_restores() {
    let d = dir("ticked");
    let mut open = project(&d.join("main.ohl"), &main_route());
    drop(project(&d.join("branch.ohl"), &branch()));
    let other = d.join("branch.ohl");
    let mut options = JoinOptions::new("B");
    options.merge.insert(("T3".into(), "T3".into()));
    let p = plan(&open, &other, &options);
    assert!(p.can_apply(), "{:?}", p.blocked);
    match &p.twins[0].state {
        TwinState::Merged(how) => {
            assert!(how.iter().any(|h| h.contains("section S3")), "{how:?}");
        }
        s => panic!("{s:?}"),
    }
    let before = open.export();
    let libs_before = libs(&open);
    join::join(&mut open, "ben", &p).unwrap();
    let l = open.export();
    assert_eq!(l.structures.len(), 7, "one shared tower");
    assert!(l.structure("B-T3").is_none());
    let s3 = l.sections.iter().find(|s| s.id == "S3").unwrap();
    assert_eq!(
        (s3.points[0].structure.as_str(), s3.points[0].set),
        ("T3", 2)
    );
    assert_eq!(
        l.alignment_point("B-P2").unwrap().structure.as_deref(),
        Some("T3")
    );
    // T3 is on both routes: a shared tower.
    let routes = l.structure_routes();
    let on = routes.get("T3").unwrap();
    assert!(
        on.contains(&"main".to_owned()) && on.contains(&"branch".to_owned()),
        "{on:?}"
    );
    // The same structures and sections as the line before the split.
    let whole = example();
    assert_eq!(l.structure("T3"), whole.structure("T3"));
    // Undo: exactly the project before.
    open.undo("ben").unwrap();
    assert_eq!(open.export(), before);
    assert_eq!(libs(&open), libs_before);
}

/// A label the kept tower has not: that twin refused and listed, two
/// towers; nothing guessed.
#[test]
fn a_label_mismatch_is_refused_and_listed() {
    let d = dir("mismatch");
    let open = project(&d.join("main.ohl"), &main_route());
    let mut b = branch();
    let t3 = b.structures.iter_mut().find(|s| s.id == "T3").unwrap();
    for w in &mut t3.wires {
        if w.label == "W2" {
            w.label = "W9".into();
        }
    }
    for s in &mut t3.strings {
        if s.wire == "W2" {
            s.wire = "W9".into();
        }
    }
    drop(project(&d.join("branch.ohl"), &b));
    let mut options = JoinOptions::new("B");
    options.merge.insert(("T3".into(), "T3".into()));
    let p = plan(&open, &d.join("branch.ohl"), &options);
    match &p.twins[0].state {
        TwinState::Refused(why) => {
            assert!(
                why.iter().any(|w| w.contains("no wire point labelled W9")),
                "{why:?}"
            );
        }
        s => panic!("{s:?}"),
    }
    assert!(p.can_apply(), "the rest joins: {:?}", p.blocked);
    assert_eq!(p.line.structures.len(), 8, "two towers");
    assert!(join::twin_words(&p.twins[0]).contains("refused"));
}

/// Library entries: the same reused, a new one added, a differing one a
/// conflict that refuses the join until accepted.
#[test]
fn library_entries_are_mapped_as_the_import_maps_them() {
    let d = dir("library");
    let mut open = project(&d.join("main.ohl"), &main_route());
    open.put_library_file(
        "ana",
        "cables",
        "lib.ohlw",
        "[[cable]]\nid = \"X\"\nmass = 1.0\n",
    )
    .unwrap();
    let mut other = project(&d.join("branch.ohl"), &branch());
    other
        .put_library_file(
            "ana",
            "cables",
            "lib.ohlw",
            "[[cable]]\nid = \"X\"\nmass = 1.0\n\n[[cable]]\nid = \"Y\"\nmass = 2.0\n",
        )
        .unwrap();
    other
        .put_library_file(
            "ana",
            "cables",
            "more.ohlw",
            "[[cable]]\nid = \"X\"\nmass = 3.0\n",
        )
        .unwrap();
    drop(other);
    let path = d.join("branch.ohl");
    let p = plan(&open, &path, &JoinOptions::new("B"));
    let outcome = |file: &str, entry: &str| {
        p.libraries
            .iter()
            .find(|m| m.file == file && m.entry == entry)
            .map(|m| m.outcome.clone())
            .unwrap()
    };
    assert_eq!(outcome("lib.ohlw", "X"), Outcome::Same("lib.ohlw".into()));
    assert_eq!(outcome("lib.ohlw", "Y"), Outcome::Added("lib.ohlw".into()));
    assert_eq!(
        outcome("more.ohlw", "X"),
        Outcome::Conflict {
            file: "lib.ohlw".into(),
            accepted: false
        }
    );
    assert!(!p.can_apply(), "a conflict refuses the join");
    assert!(join::join(&mut open, "ben", &p).is_err());
    // Accepted: the open project's entry kept, Y added to its file.
    let mut options = JoinOptions::new("B");
    options.accept.insert("cables/more.ohlw/X".into());
    let p = plan(&open, &path, &options);
    assert!(p.can_apply(), "{:?}", p.blocked);
    assert_eq!(p.library_changes.len(), 1);
    let (kind, name, text) = &p.library_changes[0];
    assert_eq!((kind.as_str(), name.as_str()), ("cables", "lib.ohlw"));
    assert!(
        text.contains("\"Y\"") && text.contains("mass = 1.0"),
        "{text}"
    );
    let before = libs(&open);
    join::join(&mut open, "ben", &p).unwrap();
    let after = open.library_files("cables").unwrap();
    assert_eq!(after.len(), 1);
    assert!(after[0].1.contains("\"Y\""));
    open.undo("ben").unwrap();
    assert_eq!(
        libs(&open),
        before,
        "one undo step takes the files back too"
    );
}

/// An older project file (schema 4) is read in memory, left unchanged.
#[test]
fn an_older_file_is_read_unchanged() {
    let d = dir("older");
    let open = project(&d.join("main.ohl"), &main_route());
    drop(project(&d.join("branch.ohl"), &branch()));
    let path = d.join("branch.ohl");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("UPDATE ols_meta SET value = '4' WHERE key = 'session_schema';")
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let p = plan(&open, &path, &JoinOptions::new("B"));
    assert!(p.can_apply(), "{:?}", p.blocked);
    assert_eq!(p.objects.get("sections"), Some(&1));
    assert_eq!(std::fs::read(&path).unwrap(), bytes, "never changed");
}

/// Refused: another coordinate system; a bad prefix.
#[test]
fn another_coordinate_system_and_bad_prefixes_are_refused() {
    let d = dir("refused");
    let open = project(&d.join("main.ohl"), &main_route());
    let mut b = branch();
    b.crs = "EPSG:4326".into();
    drop(project(&d.join("branch.ohl"), &b));
    let e = join::plan(
        &open.export(),
        &[],
        &d.join("branch.ohl"),
        &JoinOptions::new("B"),
    )
    .unwrap_err();
    assert!(e.to_string().contains("reproject"), "{e}");
    let e = join::plan(
        &open.export(),
        &[],
        &d.join("branch.ohl"),
        &JoinOptions::new("a b"),
    )
    .unwrap_err();
    assert!(e.to_string().contains("code"), "{e}");
}
