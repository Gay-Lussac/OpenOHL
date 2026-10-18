// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Replace structure ([`crate::replace`]) on the typed synthetic line:
//! T2 is a `type-02` (sets 1 and 11), T3 a `type-03` (sets 1, 11 and 2;
//! S3 and the jumper J-T3-branch at set 2).

use super::replace::{NewWire, SetMove};
use super::*;

fn typed() -> Line {
    Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line-typed.ohl.json"),
    )
    .unwrap()
}

fn project(name: &str, line: &Line) -> Session {
    let dir =
        std::env::temp_dir().join(format!("ols-session-{}-replace-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    std::fs::remove_file(&path).ok();
    let mut s = Session::open(&path).unwrap();
    s.import(line, "ana").unwrap();
    s.set_user("ana", "ben", Role::Designer).unwrap();
    s
}

/// A structure of type `t` (no extension), nothing of its own.
fn of_type(t: &str) -> Box<Structure> {
    let mut s = typed().structure("T2").unwrap().clone();
    s.structure_type = Some(t.into());
    s.extension = None;
    Box::new(s)
}

/// The wire points of the example's types as placed.
fn wires(sets: &[u32]) -> Vec<NewWire> {
    sets.iter()
        .map(|&set| NewWire {
            label: format!("W{set}"),
            set: Some(set),
        })
        .collect()
}

fn replace(id: &str, t: &str, sets: &[u32], moves: &[SetMove], confirm: bool) -> Command {
    Command::ReplaceStructure {
        id: id.into(),
        structure: of_type(t),
        wires: wires(sets),
        sets: moves.to_vec(),
        confirm,
    }
}

fn points_at(s: &Session, section: &str, structure: &str) -> Vec<u32> {
    s.line()
        .sections
        .iter()
        .find(|x| x.id == section)
        .unwrap()
        .points
        .iter()
        .filter(|p| p.structure == structure)
        .map(|p| p.set)
        .collect()
}

#[test]
fn every_set_matching_rewires_without_a_question_and_undoes() {
    let line = typed();
    let mut s = project("match", &line);
    let before = s.export();
    let t2 = before.structure("T2").unwrap().clone();
    // type-02 (1, 11) by type-03 (1, 11, 2): nothing goes.
    let applied = s
        .apply("ben", replace("T2", "type-03", &[1, 11, 2], &[], false))
        .unwrap();
    let after = s.line().structure("T2").unwrap().clone();
    assert_eq!(after.structure_type.as_deref(), Some("type-03"));
    assert_eq!(
        (after.position, after.orientation_deg, after.height_adjust),
        (t2.position, t2.orientation_deg, t2.height_adjust)
    );
    assert_eq!(points_at(&s, "S1", "T2"), vec![1]);
    assert_eq!(points_at(&s, "S2", "T2"), vec![11]);
    // The sections through it are solved again.
    assert!(applied.sections.contains(&"S1".to_owned()));
    let note = s.log().unwrap().last().unwrap().note.clone().unwrap();
    assert!(note.contains("type-02 by type-03"), "{note}");
    // One undoable entry: undo, redo.
    s.undo("ben").unwrap();
    assert_eq!(s.export(), before);
    s.redo("ben").unwrap();
    assert_eq!(
        s.line().structure("T2").unwrap().structure_type.as_deref(),
        Some("type-03")
    );
    // The command reads back from its JSON (the log, the server).
    let c = replace(
        "T2",
        "type-02",
        &[1, 11],
        &[SetMove { from: 2, to: None }],
        true,
    );
    let back: Command = serde_json::from_value(serde_json::to_value(&c).unwrap()).unwrap();
    assert_eq!(back, c);
}

#[test]
fn a_mapping_moves_section_ends_to_other_sets() {
    let mut s = project("map", &typed());
    let swap = [
        SetMove {
            from: 1,
            to: Some(11),
        },
        SetMove {
            from: 11,
            to: Some(1),
        },
    ];
    s.apply("ben", replace("T2", "type-02", &[1, 11], &swap, false))
        .unwrap();
    assert_eq!(points_at(&s, "S1", "T2"), vec![11]);
    assert_eq!(points_at(&s, "S2", "T2"), vec![1]);
    // A set the new structure does not have, or two old sets to one new
    // set: refused.
    let to_missing = [SetMove {
        from: 1,
        to: Some(5),
    }];
    let err = s
        .apply("ben", replace("T2", "type-02", &[1, 11], &to_missing, true))
        .unwrap_err();
    assert!(
        err.to_string().contains("no wire point with set 5"),
        "{err}"
    );
    let both = [SetMove {
        from: 11,
        to: Some(1),
    }];
    let err = s
        .apply("ben", replace("T2", "type-02", &[1, 11], &both, true))
        .unwrap_err();
    assert!(err.to_string().contains("both to new set 1"), "{err}");
}

#[test]
fn an_unmapped_set_removes_its_sections_after_a_confirmation() {
    let line = typed();
    let mut s = project("remove", &line);
    let before = s.export();
    // type-03 (1, 11, 2) by type-02 (1, 11): S3 and the jumper at set 2
    // go.
    let Err(SessionError::NeedsConfirmation(i)) =
        s.apply("ben", replace("T3", "type-02", &[1, 11], &[], false))
    else {
        panic!("confirmation expected");
    };
    assert_eq!(i.sections, vec!["S3"]);
    assert_eq!(i.jumpers, vec!["J-T3-branch"]);
    assert_eq!(
        s.export(),
        before,
        "nothing changed before the confirmation"
    );
    // Plan only: what the window lists.
    let plan = s
        .replacement("T3", &of_type("type-02"), &wires(&[1, 11]), &[])
        .unwrap();
    assert_eq!(plan.moves, vec![(1, Some(1)), (2, None), (11, Some(11))]);
    assert_eq!(plan.rewired(), 2);
    s.apply("ben", replace("T3", "type-02", &[1, 11], &[], true))
        .unwrap();
    assert!(s.line().sections.iter().all(|x| x.id != "S3"));
    assert!(s.line().jumpers.is_empty());
    let note = s.log().unwrap().last().unwrap().note.clone().unwrap();
    assert!(
        note.contains("S3") && note.contains("J-T3-branch"),
        "{note}"
    );
    s.undo("ben").unwrap();
    assert_eq!(s.export(), before);
    // Given "none" explicitly: the same.
    let none = [SetMove { from: 1, to: None }];
    let Err(SessionError::NeedsConfirmation(i)) =
        s.apply("ben", replace("T2", "type-02", &[1, 11], &none, false))
    else {
        panic!("confirmation expected");
    };
    assert_eq!(i.sections, vec!["S1"]);
}

#[test]
fn the_old_structures_overrides_are_listed_and_dropped() {
    let mut line = typed();
    line.structures
        .iter_mut()
        .find(|x| x.id == "T2")
        .unwrap()
        .omit_sets = vec![7];
    let mut s = project("overrides", &line);
    let Err(SessionError::NeedsConfirmation(i)) =
        s.apply("ben", replace("T2", "type-02", &[1, 11], &[], false))
    else {
        panic!("confirmation expected");
    };
    assert_eq!(i.overrides, vec!["left out: set 7"]);
    assert!(i.sections.is_empty());
    s.apply("ben", replace("T2", "type-02", &[1, 11], &[], true))
        .unwrap();
    assert!(s.line().structure("T2").unwrap().omit_sets.is_empty());
}
