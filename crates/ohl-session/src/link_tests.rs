// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Attachment points linked to joints (2026-10-21): link, relink and
//! unlink, each one undoable command; several relinked at once; a
//! replaced structure carrying its old points, linked or removed.

use super::replace::{NewWire, PointMove};
use super::*;

fn line(name: &str) -> Line {
    Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line")
            .join(name),
    )
    .unwrap()
}

fn project(name: &str, line: &Line) -> Session {
    let dir = std::env::temp_dir().join(format!("ohl-session-{}-links-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    std::fs::remove_file(&path).ok();
    let mut s = Session::open(&path).unwrap();
    s.import(line, "ana").unwrap();
    s.set_user("ana", "ben", Role::Designer).unwrap();
    s
}

fn json_back(c: &Command) -> Command {
    serde_json::from_str(&serde_json::to_string(c).unwrap()).unwrap()
}

/// Link (and relink) and unlink: each the structure's own point, one log
/// entry, undone to the bytes before.
#[test]
fn link_relink_unlink_each_undo() {
    let mut s = project("each", &line("synthetic-line-typed.ohl.json"));
    let before = s.export().to_json();
    let link = Command::LinkPoint {
        structure: "T2".into(),
        label: "A1".into(),
        joint: "A1".into(),
        offset: [0.0, 0.5, 0.0],
        position: [10.0, 20.0, 530.0],
    };
    assert_eq!(json_back(&link), link);
    s.apply("ben", link).unwrap();
    let p = s
        .line()
        .structure("T2")
        .unwrap()
        .point("A1")
        .unwrap()
        .clone();
    assert_eq!(
        (p.joint.as_deref(), p.offset, p.position, p.unlinked),
        (Some("A1"), [0.0, 0.5, 0.0], [10.0, 20.0, 530.0], false)
    );
    let linked = s.export().to_json();
    // Relinked to another joint: asks nothing, one entry.
    let relink = Command::LinkPoint {
        structure: "T2".into(),
        label: "A1".into(),
        joint: "A2".into(),
        offset: [0.0; 3],
        position: [11.0, 20.0, 530.0],
    };
    s.apply("ben", relink).unwrap();
    let p = s
        .line()
        .structure("T2")
        .unwrap()
        .point("A1")
        .unwrap()
        .clone();
    assert_eq!(p.joint.as_deref(), Some("A2"));
    assert_eq!(s.line().structure("T2").unwrap().points.len(), 1);
    let unlink = Command::UnlinkPoint {
        structure: "T2".into(),
        label: "A1".into(),
        position: [11.0, 20.0, 530.0],
    };
    assert_eq!(json_back(&unlink), unlink);
    s.apply("ben", unlink).unwrap();
    let p = s
        .line()
        .structure("T2")
        .unwrap()
        .point("A1")
        .unwrap()
        .clone();
    assert!(p.unlinked && p.joint.is_none());
    let note = s.log().unwrap().last().unwrap().note.clone().unwrap();
    assert!(note.contains("unlink attachment point A1"), "{note}");
    s.undo("ben").unwrap();
    s.undo("ben").unwrap();
    assert_eq!(s.export().to_json(), linked);
    s.undo("ben").unwrap();
    assert_eq!(s.export().to_json(), before);
}

/// Several points relinked at once (after a type revision): one entry.
/// A structure without a type has no joints to link to: refused.
#[test]
fn relink_several_and_refuse_untyped() {
    let mut s = project("several", &line("synthetic-line-typed.ohl.json"));
    let before = s.export().to_json();
    let point = |label: &str, joint: &str| ohl_line::StructurePoint {
        joint: Some(joint.into()),
        ..ohl_line::StructurePoint::at(label, [1.0, 2.0, 3.0])
    };
    let c = Command::RelinkPoints {
        structure: "T2".into(),
        points: vec![point("A1", "A1"), point("A11", "A2")],
    };
    assert_eq!(json_back(&c), c);
    s.apply("ben", c).unwrap();
    assert_eq!(s.line().structure("T2").unwrap().points.len(), 2);
    s.undo("ben").unwrap();
    assert_eq!(s.export().to_json(), before);
    // A wire point's label is not an attachment point's.
    let bad = Command::RelinkPoints {
        structure: "T2".into(),
        points: vec![ohl_line::StructurePoint::at("", [0.0; 3])],
    };
    assert!(s.apply("ben", bad).is_err());

    let mut u = project("untyped", &line("synthetic-line.ohl.json"));
    let err = u
        .apply(
            "ben",
            Command::LinkPoint {
                structure: "T2".into(),
                label: "A1".into(),
                joint: "A1".into(),
                offset: [0.0; 3],
                position: [0.0; 3],
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("no type"), "{err}");
}

/// Replace structure with the old attachment points matched: one carried
/// (the new structure holds it, linked), one removed — listed, asked
/// for, done in one entry and undone. A point said to be linked that the
/// new structure does not carry so is refused.
#[test]
fn replace_carries_or_removes_points() {
    let typed = line("synthetic-line-typed.ohl.json");
    let mut s = project("replace", &typed);
    let before = s.export().to_json();
    let mut new = typed.structure("T2").unwrap().clone();
    new.structure_type = Some("type-03".into());
    let wires: Vec<NewWire> = [1, 11, 2]
        .iter()
        .map(|&set| NewWire {
            label: format!("W{set}"),
            set: Some(set),
        })
        .collect();
    let moves = vec![
        PointMove {
            label: "A1".into(),
            joint: Some("A1".into()),
            offset: [0.0, 0.2, 0.0],
        },
        PointMove {
            label: "A11".into(),
            joint: None,
            offset: [0.0; 3],
        },
    ];
    let command = |new: &Structure, confirm: bool| Command::ReplaceStructure {
        id: "T2".into(),
        structure: Box::new(new.clone()),
        wires: wires.clone(),
        sets: Vec::new(),
        points: moves.clone(),
        confirm,
    };
    // The new structure does not carry A1: refused.
    assert!(s.apply("ben", command(&new, true)).is_err());
    new.points.push(ohl_line::StructurePoint {
        joint: Some("A1".into()),
        offset: [0.0, 0.2, 0.0],
        ..ohl_line::StructurePoint::at("A1", [1.0, 2.0, 3.0])
    });
    let c = command(&new, false);
    assert_eq!(json_back(&c), c);
    match s.apply("ben", c) {
        Err(SessionError::NeedsConfirmation(impact)) => {
            assert!(
                impact
                    .overrides
                    .iter()
                    .any(|o| o == "attachment point A11 (removed)"),
                "{impact:?}"
            );
        }
        other => panic!("{other:?}"),
    }
    s.apply("ben", command(&new, true)).unwrap();
    let t2 = s.line().structure("T2").unwrap();
    assert_eq!(t2.structure_type.as_deref(), Some("type-03"));
    assert_eq!(t2.point("A1").unwrap().joint.as_deref(), Some("A1"));
    s.undo("ben").unwrap();
    assert_eq!(s.export().to_json(), before);
}
