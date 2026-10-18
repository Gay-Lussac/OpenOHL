// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use std::path::PathBuf;

fn example() -> Line {
    Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap()
}

fn project(name: &str) -> (PathBuf, Session) {
    let dir = std::env::temp_dir().join(format!("ohl-session-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
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
    // Its points moved with it.
    assert_eq!(
        t6.points[0].position[0] - t6.position[0],
        example().structure("T6").unwrap().points[0].position[0] - p[0]
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
    // T3 is the Y pylon: the branch's point tied to it is detached.
    let y = s
        .line()
        .points_of(s.line().alignment_by_id("branch").unwrap())[0]
        .id
        .clone();
    assert_eq!(impact.alignment_detached, vec![y]);
    assert!(impact.alignments_deleted.is_empty());
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
            .alignment_points
            .iter()
            .all(|p| p.structure.as_deref() != Some("T3"))
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
fn deleting_a_route_lists_its_alignments_and_an_alignment_may_lose_every_point() {
    let (_, mut s) = project("route");
    // A route: its alignments on no other route go with it (asked).
    let del = |confirm| Command::Delete {
        kind: Kind::Route,
        id: "branch".into(),
        confirm,
    };
    let Err(SessionError::NeedsConfirmation(i)) = s.apply("ben", del(false)) else {
        panic!("confirmation expected");
    };
    assert_eq!(i.alignments_deleted, vec!["branch"]);
    s.apply("ben", del(true)).unwrap();
    assert!(s.line().alignments.iter().all(|a| a.id != "branch"));
    assert!(s.line().routes.iter().all(|r| r.id != "branch"));
    s.undo("ben").unwrap();
    assert_eq!(s.export(), example());
    // An alignment stays without points (its points deleted one by one;
    // the Y pylon's point goes off main too).
    let n = s.line().alignment_by_id("branch").unwrap().points.len();
    for _ in 0..n {
        s.apply(
            "ben",
            Command::RemoveAlignmentPoint {
                route: "branch".into(),
                index: 0,
            },
        )
        .unwrap();
    }
    assert!(
        s.line()
            .alignment_by_id("branch")
            .unwrap()
            .points
            .is_empty()
    );
    assert_eq!(s.line().alignment_by_id("main").unwrap().points.len(), 2);
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
        let l = s.line();
        l.points_of(l.alignment_by_id("branch").unwrap())
    };
    assert_eq!(branch(&s).len(), 3);
    assert_eq!(branch(&s)[1].structure.as_deref(), Some("T6"));
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
    assert_eq!(branch(&s)[1].position, moved);
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
    assert!(branch(&s)[1].structure.is_none());
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
    assert_eq!(branch(&s)[1].position[0], moved[0] + 10.0);
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
    assert_eq!(branch(&s)[1].position, p);
    // A tied point away from its structure is invalid.
    let mut r = branch(&s)[1].clone();
    r.position[0] += 1.0;
    let err = s
        .apply(
            "ben",
            Command::Put {
                kind: Kind::AlignmentPoint,
                id: r.id.clone(),
                value: serde_json::to_value(&r).unwrap(),
                ord: None,
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("tied to T6"), "{err}");
    // Deleting T7: its point is detached (listed), the branch keeps T3–T6.
    let impact = s.structure_deletion("T7").unwrap();
    assert_eq!(impact.alignment_detached, vec![branch(&s)[2].id.clone()]);
    s.apply(
        "ben",
        Command::DeleteStructure {
            id: "T7".into(),
            confirm: true,
        },
    )
    .unwrap();
    let b = branch(&s);
    let view = s
        .line()
        .alignment_views()
        .into_iter()
        .find(|v| v.id == "branch")
        .unwrap();
    assert_eq!(view.structures, vec!["T3", "T6"]);
    assert_eq!(b.len(), 3);
    assert!(b[2].structure.is_none());
    // T3 and T6 tied (T6 was re-tied above), T7's former point free.
    assert_eq!(b[0].structure.as_deref(), Some("T3"));
    assert_eq!(b[2].position, [640.0, 560.0, 496.0]);
}

#[test]
fn deleting_an_alignment_point_takes_it_off_every_alignment() {
    let (_, mut s) = project("delete-point");
    // The Y pylon's point is on main and on the branch.
    let shared = s
        .line()
        .alignment_points
        .iter()
        .find(|p| s.line().point_alignments(&p.id).len() == 2)
        .unwrap()
        .id
        .clone();
    let impact = s.deletion_impact(Kind::AlignmentPoint, &shared).unwrap();
    assert_eq!(impact.alignments_changed, ["main", "branch"]);
    let before: Vec<Vec<String>> = s
        .line()
        .alignments
        .iter()
        .map(|a| a.points.clone())
        .collect();
    let delete = |confirm| Command::Delete {
        kind: Kind::AlignmentPoint,
        id: shared.clone(),
        confirm,
    };
    assert!(matches!(
        s.apply("ben", delete(false)),
        Err(SessionError::NeedsConfirmation(_))
    ));
    s.apply("ben", delete(true)).unwrap();
    for (a, b) in s.line().alignments.iter().zip(&before) {
        let kept: Vec<String> = b.iter().filter(|p| **p != shared).cloned().collect();
        assert_eq!(a.points, kept);
    }
    assert!(s.line().alignment_point(&shared).is_none());
    s.undo("ben").unwrap();
    assert_eq!(s.export(), example());
    // By index (the tools): the same.
    let k = s
        .line()
        .alignment_by_id("branch")
        .unwrap()
        .points
        .iter()
        .position(|p| *p == shared)
        .unwrap();
    s.apply(
        "ben",
        Command::RemoveAlignmentPoint {
            route: "branch".into(),
            index: k,
        },
    )
    .unwrap();
    assert!(s.line().point_alignments(&shared).is_empty());
    // Deleting an alignment keeps its points (waiting on no alignment).
    s.undo("ben").unwrap();
    let only_branch: Vec<String> = s
        .line()
        .alignment_by_id("branch")
        .unwrap()
        .points
        .iter()
        .filter(|p| **p != shared)
        .cloned()
        .collect();
    let impact = s.deletion_impact(Kind::Alignment, "branch").unwrap();
    assert!(!impact.has_dependents());
    s.apply(
        "ben",
        Command::Delete {
            kind: Kind::Alignment,
            id: "branch".into(),
            confirm: true,
        },
    )
    .unwrap();
    assert!(s.line().alignment_point(&shared).is_some());
    assert!(
        only_branch
            .iter()
            .all(|p| s.line().alignment_point(p).is_some())
    );
    assert!(s.line().point_alignments(&only_branch[0]).is_empty());
}

#[test]
fn a_junction_shares_the_point() {
    let (_, mut s) = project("junction");
    // A new alignment of the main route from main's last point.
    let last = s
        .line()
        .alignment_by_id("main")
        .unwrap()
        .points
        .last()
        .unwrap()
        .clone();
    let mut a = ohl_line::Alignment::new("spur", "main", Vec::new());
    a.name = "Spur".into();
    s.apply(
        "ben",
        Command::Put {
            kind: Kind::Alignment,
            id: "spur".into(),
            value: serde_json::to_value(&a).unwrap(),
            ord: None,
        },
    )
    .unwrap();
    s.apply(
        "ben",
        Command::InsertAlignmentPoint {
            route: "spur".into(),
            index: 0,
            position: [0.0; 3],
            point: Some(last.clone()),
        },
    )
    .unwrap();
    s.apply(
        "ben",
        Command::InsertAlignmentPoint {
            route: "spur".into(),
            index: 1,
            position: [1500.0, 300.0, 500.0],
            point: None,
        },
    )
    .unwrap();
    let l = s.line();
    assert_eq!(l.point_alignments(&last), ["main", "spur"]);
    assert_eq!(l.alignment_by_id("spur").unwrap().points.len(), 2);
    // Moving the shared point moves it on both.
    s.apply(
        "ben",
        Command::MoveAlignmentPoint {
            route: "spur".into(),
            index: 0,
            position: [1250.0, 110.0, 503.0],
        },
    )
    .unwrap();
    let l = s.line();
    assert_eq!(
        l.points_of(l.alignment_by_id("main").unwrap())
            .last()
            .unwrap()
            .position,
        [1250.0, 110.0, 503.0]
    );
    // An alignment on an unknown route is refused.
    let bad = ohl_line::Alignment::new("nowhere", "none", Vec::new());
    assert!(
        s.apply(
            "ben",
            Command::Put {
                kind: Kind::Alignment,
                id: "nowhere".into(),
                value: serde_json::to_value(&bad).unwrap(),
                ord: None,
            },
        )
        .is_err()
    );
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
            (Kind::Alignment, "branch".to_owned()),
            (Kind::AlignmentPoint, "P4".to_owned()),
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
            (Kind::Alignment, "branch".to_owned()),
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
        phase: None,
        confirm,
    };
    let Err(SessionError::NeedsConfirmation(i)) = s.apply("ben", del(false)) else {
        panic!("confirmation expected");
    };
    assert_eq!(i.what(), "structure T3 set 2");
    assert_eq!(i.sections, vec!["S3"]);
    assert_eq!(i.jumpers, vec!["J-T3-branch"]);
    let applied = s.apply("ben", del(true)).unwrap();
    let t3 = s.line().structure("T3").unwrap();
    assert!(t3.wire_at_set(2).is_none());
    assert!(
        t3.omit.is_empty() && t3.omit_sets.is_empty(),
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
    assert!(note.contains("set 2") && note.contains("S3"), "{note}");
    s.undo("ben").unwrap();
    assert_eq!(s.export(), example());
    // A string nothing hangs on goes without a confirmation; one that is
    // not there is refused.
    s.apply("ben", del(true)).unwrap();
    assert!(s.apply("ben", del(false)).is_err());
}

#[test]
fn a_typed_structure_leaves_out_a_string_of_its_type() {
    let dir = std::env::temp_dir().join(format!("ohl-session-{}-typed", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    std::fs::remove_file(&path).ok();
    let ex = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
    let line = Line::load(&ex.join("synthetic-line-typed.ohl.json")).unwrap();
    let mut s = Session::open(&path).unwrap();
    s.import(&line, "ana").unwrap();
    s.apply(
        "ana",
        Command::DeleteAttachment {
            structure: "T3".into(),
            set: 2,
            // As projects written before give it.
            phase: Some(1),
            confirm: true,
        },
    )
    .unwrap();
    let t3 = s.line().structure("T3").unwrap();
    assert_eq!(t3.omit_sets, vec![2]);
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
    assert_eq!(l.jumpers[0].from.structure(), Some("Y3"));
    // The points tied to it follow the new name; it stays on its routes.
    assert!(
        l.alignment_points
            .iter()
            .all(|p| p.structure.as_deref() != Some("T3"))
    );
    assert_eq!(l.routes_of("Y3"), ["branch", "main"]);
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
            phase: None,
            new_set: 5,
            new_phase: None,
        },
    )
    .unwrap();
    let p = &s.line().sections[2].points[0];
    assert_eq!((p.structure.as_str(), p.set), ("T3", 5));
    assert_eq!(s.line().jumpers[0].from.point().unwrap().set, 5);
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
    s.set_setting("app.weather", Some("/x/cases.ohlwc"))
        .unwrap();
    s.set_setting("app.weather", Some("/y/cases.ohlwc"))
        .unwrap();
    drop(s);
    let again = Session::open(&path).unwrap();
    assert_eq!(
        again.setting("app.weather").unwrap().as_deref(),
        Some("/y/cases.ohlwc")
    );
    again.set_setting("app.weather", None).unwrap();
    assert_eq!(again.setting("app.weather").unwrap(), None);
    assert_eq!(again.export(), example(), "not part of the line");
}

#[test]
fn library_files_are_kept_in_the_project() {
    let (path, mut s) = project("libraries");
    assert!(s.library_files("cables").unwrap().is_empty());
    s.put_library_file("ben", "cables", "a.ohlw", "x = 1")
        .unwrap();
    s.put_library_file("ben", "cables", "b.ohlw", "y = 2")
        .unwrap();
    s.put_library_file("ben", "cables", "a.ohlw", "x = 3")
        .unwrap();
    s.put_library_file("ben", "weather", "w.ohlwc", "z = 4")
        .unwrap();
    // Viewers cannot; names are file names.
    assert!(s.put_library_file("vic", "cables", "c.ohlw", "").is_err());
    assert!(
        s.put_library_file("ben", "cables", "../c.ohlw", "")
            .is_err()
    );
    drop(s);
    let mut s = Session::open(&path).unwrap();
    assert_eq!(
        s.library_files("cables").unwrap(),
        vec![
            ("a.ohlw".to_owned(), "x = 3".to_owned()),
            ("b.ohlw".to_owned(), "y = 2".to_owned())
        ]
    );
    s.delete_library_file("ben", "cables", "a.ohlw").unwrap();
    assert_eq!(s.library_files("cables").unwrap().len(), 1);
    assert_eq!(s.library_files("weather").unwrap().len(), 1);
    // Logged: undone as any change.
    s.undo("ben").unwrap();
    assert_eq!(s.library_files("cables").unwrap().len(), 2);
}

/// A project of schema 3 kept its library files in one table for all its
/// lines: each line gets them as objects when it opens.
#[test]
fn library_files_of_schema_3_become_each_lines() {
    let (path, s) = project("libraries-3");
    drop(s);
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS library_file (
                 kind TEXT NOT NULL, name TEXT NOT NULL,
                 time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                 user TEXT NOT NULL, text TEXT NOT NULL, PRIMARY KEY (kind, name));
             INSERT INTO library_file (kind, name, user, text) VALUES ('cables', 'old.ohlw', 'ana', 'x = 1');
             UPDATE ols_meta SET value = '3' WHERE key = 'session_schema';",
        )
        .unwrap();
    }
    let s = Session::open(&path).unwrap();
    assert_eq!(
        s.library_files("cables").unwrap(),
        vec![("old.ohlw".to_owned(), "x = 1".to_owned())]
    );
    assert!(!has_table(&path, "library_file"));
}

fn has_table(path: &std::path::Path, name: &str) -> bool {
    Connection::open(path)
        .unwrap()
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")
        .unwrap()
        .exists([name])
        .unwrap()
}

/// A new project file has only the current tables, with every column a
/// migration would add.
#[test]
fn a_new_project_has_no_tables_of_older_schemas() {
    let (path, s) = project("fresh");
    drop(s);
    assert!(!has_table(&path, "library_file"));
    assert!(!has_table(&path, "line_snapshot"));
    let c = Connection::open(&path).unwrap();
    for (table, column) in [
        ("oplog", "note"),
        ("oplog", "line"),
        ("model_object", "line"),
        ("vc_ref", "description"),
    ] {
        let has: bool = c
            .prepare("SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2")
            .unwrap()
            .exists([table, column])
            .unwrap();
        assert!(has, "{table}.{column}");
    }
    drop(c);
    // Opening again changes nothing.
    Session::open(&path).unwrap();
    assert!(!has_table(&path, "line_snapshot"));
}

#[test]
fn library_file_names_are_plain() {
    for good in [
        "cables.ohlw",
        "type-01.ohls",
        "I-0.10m.ohli",
        "Zürich (2).ohls",
    ] {
        assert!(crate::safe_file_name(good), "{good}");
    }
    for bad in [
        "",
        "..",
        ".hidden",
        "a/b",
        "a\\b",
        "C:evil.ohlw",
        "x:stream",
        "CON.ohlw",
        "lpt1",
        "end.",
        "end ",
    ] {
        assert!(!crate::safe_file_name(bad), "{bad}");
    }
}

#[test]
fn reprojecting_moves_line_saved_lines_and_surfaces() {
    let (_path, mut s) = project("reproject");
    s.create_line("ana", "as built", "main").unwrap();
    s.put_surface(
        "ana",
        "ground",
        "test",
        &[[0.0, 0.0, 1.0], [10.0, 0.0, 2.0], [0.0, 10.0, 3.0]],
        &[[0, 1, 2]],
    )
    .unwrap();
    let before = s.line().clone();
    let shift = |pts: &mut [[f64; 3]]| -> Result<(), String> {
        for p in pts {
            p[0] += 1000.0;
            p[1] -= 500.0;
        }
        Ok(())
    };
    assert!(
        s.reproject("ben", "EPSG:32632", &shift).is_err(),
        "admins only"
    );
    s.reproject("ana", "EPSG:32632", &shift).unwrap();
    let moved = s.line().clone();
    assert_eq!(moved.crs, "EPSG:32632");
    for (a, b) in before.structures.iter().zip(&moved.structures) {
        assert_eq!(
            b.position,
            [a.position[0] + 1000.0, a.position[1] - 500.0, a.position[2]]
        );
        assert!((a.orientation_deg - b.orientation_deg).abs() < 1e-9);
    }
    let saved = s.line_named("as built").unwrap().clone();
    assert_eq!(saved.crs, "EPSG:32632");
    assert_eq!(saved.structures[0].position, moved.structures[0].position);
    let (points, tris) = s.surface("ground").unwrap();
    assert_eq!(points[1], [1010.0, -500.0, 2.0]);
    assert_eq!(tris, vec![[0, 1, 2]]);
    // Not undone: the terrain would not follow.
    assert!(s.undo("ana").is_err());
    let last = s.log().unwrap().last().unwrap().seq;
    assert!(s.rollback("ana", last).is_err());
    // A transform that fails leaves the project as it was.
    let fail = |_: &mut [[f64; 3]]| -> Result<(), String> { Err("no image".into()) };
    assert!(s.reproject("ana", "EPSG:4326", &fail).is_err());
    assert_eq!(s.line(), &moved);
}

/// A project from before routes and alignments (its routes each their own
/// alignment, a route following its structures, no record of the
/// change) is read into routes and alignments once, when it opens; a
/// project of the current form is left alone, even with structures on no
/// alignment.
#[test]
fn old_projects_get_routes_and_alignments_when_they_open() {
    let (path, s) = project("old-routes");
    let now = s.line().clone();
    drop(s);
    // Back to the old form, as an older build wrote it.
    {
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute(
            "DELETE FROM model_object WHERE kind IN ('alignment', 'alignment_point')",
            [],
        )
        .unwrap();
        for v in now.alignment_views() {
            let positions: Vec<[f64; 3]> = v.alignment.iter().map(|p| p.position).collect();
            let old = serde_json::json!({
                "id": v.id,
                "name": v.name,
                // The branch followed its structures (no points).
                "alignment": if v.id == "branch" { serde_json::json!([]) } else { serde_json::to_value(&positions).unwrap() },
                "structures": v.structures,
            });
            c.execute(
                "UPDATE model_object SET json = ?1 WHERE kind = 'route' AND id = ?2",
                rusqlite::params![old.to_string(), v.id],
            )
            .unwrap();
        }
        c.execute("DELETE FROM ols_meta WHERE key = 'routes_alignments'", [])
            .unwrap();
    }
    let s = Session::open(&path).unwrap();
    let l = s.line();
    assert_eq!(l.alignments.len(), 2);
    // The Y pylon's point is shared by main and the branch.
    let t3: Vec<&ohl_line::AlignmentPoint> = l
        .alignment_points
        .iter()
        .filter(|p| p.structure.as_deref() == Some("T3"))
        .collect();
    assert_eq!(t3.len(), 1);
    assert_eq!(l.point_alignments(&t3[0].id), ["main", "branch"]);
    assert!(l.routes.iter().all(|r| !r.is_legacy()));
    let branch = l.points_of(l.alignment_by_id("branch").unwrap());
    let main = l.points_of(l.alignment_by_id("main").unwrap());
    assert!(branch.iter().all(|p| p.structure.is_some()));
    assert_eq!(l.routes_of("T3"), ["branch", "main"]);
    assert_eq!(l.routes_of("T6"), ["branch"]);
    drop(s);
    // Rewritten in place: the next opening reads the new form as it is.
    let c = rusqlite::Connection::open(&path).unwrap();
    let alignments: i64 = c
        .query_row(
            "SELECT count(*) FROM model_object WHERE kind = 'alignment'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(alignments, 2);
    // Version 3: alignments holding their points (copies at the junction),
    // on one or more routes.
    let v3 = |id: &str, routes: &[&str], points: &[ohl_line::AlignmentPoint]| {
        let pts: Vec<serde_json::Value> = points
            .iter()
            .map(|p| match &p.structure {
                Some(st) => serde_json::json!({"position": p.position, "structure": st}),
                None => serde_json::json!(p.position),
            })
            .collect();
        serde_json::json!({"id": id, "name": "", "routes": routes, "points": pts})
    };
    for (id, value) in [
        ("main", v3("main", &["main"], &main)),
        ("branch", v3("branch", &["branch", "main"], &branch)),
    ] {
        c.execute(
            "UPDATE model_object SET json = ?1 WHERE kind = 'alignment' AND id = ?2",
            rusqlite::params![value.to_string(), id],
        )
        .unwrap();
    }
    c.execute_batch(
        "DELETE FROM model_object WHERE kind = 'alignment_point';
         DELETE FROM ols_meta WHERE key = 'alignment_points';",
    )
    .unwrap();
    drop(c);
    let s = Session::open(&path).unwrap();
    let l = s.line();
    let ids: Vec<(&str, &str)> = l
        .alignments
        .iter()
        .map(|a| (a.id.as_str(), a.route.as_str()))
        .collect();
    assert_eq!(
        ids,
        [
            ("main", "main"),
            ("branch", "branch"),
            ("branch (main)", "main")
        ]
    );
    let t3 = l
        .alignment_points
        .iter()
        .find(|p| p.structure.as_deref() == Some("T3"))
        .unwrap();
    assert_eq!(l.point_alignments(&t3.id).len(), 3);
    assert_eq!(l.routes_of("T6"), ["branch", "main"]);
    drop(s);
    // A project of the current form with structures and no alignment
    // stays so (no route "main" made for it).
    let dir = std::env::temp_dir().join(format!("ohl-session-{}-no-alignment", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    std::fs::remove_file(&path).ok();
    let mut line = example();
    line.routes.clear();
    line.alignments.clear();
    line.alignment_points.clear();
    let mut s = Session::open(&path).unwrap();
    s.import(&line, "ana").unwrap();
    drop(s);
    let s = Session::open(&path).unwrap();
    assert!(s.line().routes.is_empty() && s.line().alignments.is_empty());
}

/// Spacers are objects of the project: put and undone; a section renamed
/// takes them along; a section deleted takes its spacers, after a
/// confirmation that lists them; the project's default values set as the
/// line's settings.
#[test]
fn spacers_follow_their_sections() {
    let (_p, mut s) = project("spacers");
    let mut line = s.export();
    for sec in &mut line.sections[..2] {
        sec.span_points.push(ohl_line::SpanPoint {
            label: "P1".into(),
            span: 0,
            at: ohl_line::SpanPlace::at_fraction(0.4),
            description: String::new(),
            object: Default::default(),
        });
    }
    let (s1, s2) = (line.sections[0].id.clone(), line.sections[1].id.clone());
    s.apply(
        "ana",
        Command::ReplaceLine {
            line: Box::new(line),
        },
    )
    .unwrap();
    let spacer = ohl_line::Spacer {
        id: "SP1".into(),
        a: ohl_line::SpanPointRef {
            section: s1.clone(),
            point: "P1".into(),
        },
        b: ohl_line::SpanPointRef {
            section: s2.clone(),
            point: "P1".into(),
        },
        compression: Some(10_000.0),
        ..Default::default()
    };
    s.apply("ana", put(Kind::Spacer, "SP1", &spacer)).unwrap();
    assert_eq!(s.line().spacers.len(), 1);
    s.undo("ana").unwrap();
    assert!(s.line().spacers.is_empty());
    s.redo("ana").unwrap();
    // Renamed section: the spacer follows.
    s.apply(
        "ana",
        Command::Rename {
            kind: Kind::Section,
            id: s1.clone(),
            new_id: "S1x".into(),
        },
    )
    .unwrap();
    assert_eq!(s.line().spacers[0].a.section, "S1x");
    // Deleted section: asked first, the spacer listed, then both gone.
    let err = s
        .apply(
            "ana",
            Command::Delete {
                kind: Kind::Section,
                id: s2.clone(),
                confirm: false,
            },
        )
        .unwrap_err();
    let SessionError::NeedsConfirmation(impact) = &err else {
        panic!("{err}");
    };
    assert_eq!(impact.spacers, ["SP1"]);
    s.apply(
        "ana",
        Command::Delete {
            kind: Kind::Section,
            id: s2,
            confirm: true,
        },
    )
    .unwrap();
    assert!(s.line().spacers.is_empty());
    // The default values.
    let d = ohl_line::Defaults {
        spacer_compression: 8000.0,
        insulator_stiffness: 4e7,
        ..Default::default()
    };
    s.apply("ana", s.defaults_command(d)).unwrap();
    assert_eq!(s.line().defaults, d);
}

/// Jumpers on taps follow what they hang on: a section renamed, a jumper
/// renamed; deleting a section or a jumper lists them and takes them.
#[test]
fn tapped_jumpers_follow_what_they_hang_on() {
    let (_p, mut s) = project("taps");
    let mut line = s.export();
    let s1 = line.sections[0].id.clone();
    line.sections[0].span_points.push(ohl_line::SpanPoint {
        label: "TAP".into(),
        span: 0,
        at: ohl_line::SpanPlace::at_fraction(0.4),
        description: String::new(),
        object: Some(ohl_line::LineObject::clamp(30.0)),
    });
    let host = line.jumpers[0].id.clone();
    line.jumpers[0].points.push(ohl_line::JumperPoint {
        label: "K".into(),
        at: ohl_line::JumperPlace::Fraction(0.5),
        description: String::new(),
        object: Some(ohl_line::LineObject::clamp(20.0)),
    });
    let mut a = line.jumpers[0].clone();
    a.id = "JA".into();
    a.points.clear();
    a.set_supports(Vec::new());
    a.set_support(None);
    a.sag = None;
    a.length = Some(12.0);
    a.from = ohl_line::JumperEnd::tap(ohl_line::Tap::Span(ohl_line::SpanPointRef {
        section: s1.clone(),
        point: "TAP".into(),
    }));
    let mut b = a.clone();
    b.id = "JB".into();
    b.from = ohl_line::JumperEnd::tap(ohl_line::Tap::Jumper {
        jumper: host.clone(),
        point: "K".into(),
    });
    line.jumpers.push(a);
    line.jumpers.push(b);
    s.apply(
        "ana",
        Command::ReplaceLine {
            line: Box::new(line),
        },
    )
    .unwrap();
    s.apply(
        "ana",
        Command::Rename {
            kind: Kind::Section,
            id: s1.clone(),
            new_id: "S1x".into(),
        },
    )
    .unwrap();
    let tap_of = |s: &Session, id: &str| {
        s.line()
            .jumpers
            .iter()
            .find(|j| j.id == id)
            .unwrap()
            .from
            .as_tap()
            .cloned()
    };
    assert!(matches!(tap_of(&s, "JA"), Some(ohl_line::Tap::Span(r)) if r.section == "S1x"));
    s.apply(
        "ana",
        Command::Rename {
            kind: Kind::Jumper,
            id: host.clone(),
            new_id: "Jhost".into(),
        },
    )
    .unwrap();
    assert!(
        matches!(tap_of(&s, "JB"), Some(ohl_line::Tap::Jumper { jumper, .. }) if jumper == "Jhost")
    );
    // Deleting the host jumper: asked, JB listed and gone with it.
    let err = s
        .apply(
            "ana",
            Command::Delete {
                kind: Kind::Jumper,
                id: "Jhost".into(),
                confirm: false,
            },
        )
        .unwrap_err();
    let SessionError::NeedsConfirmation(impact) = &err else {
        panic!("{err}");
    };
    assert_eq!(impact.jumpers, ["JB"]);
    s.apply(
        "ana",
        Command::Delete {
            kind: Kind::Jumper,
            id: "Jhost".into(),
            confirm: true,
        },
    )
    .unwrap();
    assert!(
        s.line()
            .jumpers
            .iter()
            .all(|j| j.id != "JB" && j.id != "Jhost")
    );
    // Deleting the section: JA with it.
    let err = s
        .apply(
            "ana",
            Command::Delete {
                kind: Kind::Section,
                id: "S1x".into(),
                confirm: false,
            },
        )
        .unwrap_err();
    let SessionError::NeedsConfirmation(impact) = &err else {
        panic!("{err}");
    };
    assert!(
        impact.jumpers.contains(&"JA".to_owned()),
        "{:?}",
        impact.jumpers
    );
}

/// A line whose section S2 has a tap TAP in its first span (T1–T2), with
/// jumper JA on it and JB on JA's point K.
fn tapped_on_s2(s: &mut Session) {
    let mut line = s.export();
    let s2 = line.sections.iter_mut().find(|x| x.id == "S2").unwrap();
    s2.span_points.push(ohl_line::SpanPoint {
        label: "TAP".into(),
        span: 0,
        at: ohl_line::SpanPlace::at_fraction(0.4),
        description: String::new(),
        object: Some(ohl_line::LineObject::clamp(30.0)),
    });
    let mut a = line.jumpers[0].clone();
    a.id = "JA".into();
    // On S2's own circuit.
    a.to = ohl_line::JumperEnd::wire("T3", 11);
    a.set_supports(Vec::new());
    a.set_support(None);
    a.sag = None;
    a.length = Some(12.0);
    a.from = ohl_line::JumperEnd::tap(ohl_line::Tap::Span(ohl_line::SpanPointRef {
        section: "S2".into(),
        point: "TAP".into(),
    }));
    a.points = vec![ohl_line::JumperPoint {
        label: "K".into(),
        at: ohl_line::JumperPlace::Fraction(0.5),
        description: String::new(),
        object: Some(ohl_line::LineObject::clamp(20.0)),
    }];
    let mut b = a.clone();
    b.id = "JB".into();
    b.points.clear();
    b.from = ohl_line::JumperEnd::tap(ohl_line::Tap::Jumper {
        jumper: "JA".into(),
        point: "K".into(),
    });
    line.jumpers.push(a);
    line.jumpers.push(b);
    s.apply(
        "ana",
        Command::ReplaceLine {
            line: Box::new(line),
        },
    )
    .unwrap();
}

/// Deleting a structure or a set lists the jumpers on taps of the
/// sections it takes (those hanging first) and deletes them.
#[test]
fn deleting_a_structure_or_a_set_takes_the_jumpers_on_taps() {
    let (_p, mut s) = project("taps-structure");
    tapped_on_s2(&mut s);
    // (With S1 goes the branch jumper at T3 too.)
    let impact = s.structure_deletion("T1").unwrap();
    assert_eq!(impact.jumpers, ["JB", "JA", "J-T3-branch"]);
    s.apply(
        "ana",
        Command::Delete {
            kind: Kind::Structure,
            id: "T1".into(),
            confirm: true,
        },
    )
    .unwrap();
    assert!(
        s.line()
            .jumpers
            .iter()
            .all(|j| j.id != "JA" && j.id != "JB")
    );
    s.undo("ana").unwrap();
    let impact = s.attachment_deletion("T1", 11).unwrap();
    assert_eq!(impact.sections, ["S2"]);
    assert_eq!(impact.jumpers, ["JB", "JA"]);
    s.apply(
        "ana",
        Command::DeleteAttachment {
            structure: "T1".into(),
            set: 11,
            phase: None,
            confirm: true,
        },
    )
    .unwrap();
    assert!(
        s.line()
            .jumpers
            .iter()
            .all(|j| j.id != "JA" && j.id != "JB")
    );
}

/// The circuit starts follow a structure renamed and a set renumbered,
/// and go with what they are on (also out of the stored settings, so
/// they do not come back with the id).
#[test]
fn circuit_starts_follow_renames_and_deletions() {
    let (_p, mut s) = project("circuit-starts");
    let mut line = s.export();
    line.circuit_starts = vec![ohl_line::SectionPoint::new("T1", 1)];
    s.apply(
        "ana",
        Command::ReplaceLine {
            line: Box::new(line),
        },
    )
    .unwrap();
    let stored = |s: &Session| -> Vec<ohl_line::SectionPoint> {
        meta_of(&s.objects).unwrap().unwrap().circuit_starts
    };
    s.apply(
        "ana",
        Command::Rename {
            kind: Kind::Structure,
            id: "T1".into(),
            new_id: "T1x".into(),
        },
    )
    .unwrap();
    assert_eq!(
        s.line().circuit_starts,
        [ohl_line::SectionPoint::new("T1x", 1)]
    );
    s.apply(
        "ana",
        Command::RenameAttachment {
            structure: "T1x".into(),
            set: 1,
            phase: None,
            new_set: 21,
            new_phase: None,
        },
    )
    .unwrap();
    assert_eq!(stored(&s), [ohl_line::SectionPoint::new("T1x", 21)]);
    s.apply(
        "ana",
        Command::Delete {
            kind: Kind::Section,
            id: "S1".into(),
            confirm: true,
        },
    )
    .unwrap();
    assert!(stored(&s).is_empty());
    assert!(s.line().circuit_starts.is_empty());
    s.undo("ana").unwrap();
    assert_eq!(
        s.line().circuit_starts,
        [ohl_line::SectionPoint::new("T1x", 21)]
    );
}

/// The line's settings the solve reads mark every section to solve
/// again; the name does not.
#[test]
fn settings_changed_mark_every_section() {
    let (_p, mut s) = project("settings-dirty");
    let mut d = s.line().defaults;
    d.jumper_stiffness_factor = 0.5;
    let a = s.apply("ana", s.defaults_command(d)).unwrap();
    assert_eq!(a.sections, ["S1", "S2", "S3"]);
    let mut line = s.line().clone();
    line.name = "renamed".into();
    let a = s.apply("ana", line_settings_put(&line, None)).unwrap();
    assert!(a.sections.is_empty());
}

/// A project file of a newer schema is refused before anything in it
/// changes.
#[test]
fn a_newer_project_file_is_left_unchanged() {
    let dir = std::env::temp_dir().join(format!("ohl-session-{}-newer", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    std::fs::remove_file(&path).ok();
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch(
            "CREATE TABLE ols_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO ols_meta (key, value) VALUES ('session_schema', '99');",
        )
        .unwrap();
    }
    let err = Session::open(&path).err().unwrap();
    assert!(err.to_string().contains("newer"), "{err}");
    let c = Connection::open(&path).unwrap();
    let tables: Vec<String> = c
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(tables, ["ols_meta"]);
}

/// Relabelling a point: the jumpers tapped on it follow (on a span, on
/// a jumper); a label taken is refused.
#[test]
fn relabelling_a_point_takes_what_names_it() {
    let (_p, mut s) = project("relabel");
    tapped_on_s2(&mut s);
    let relabel = |kind, id: &str, label: &str, new: &str| Command::RenamePoint {
        kind,
        id: id.into(),
        label: label.into(),
        new_label: new.into(),
    };
    s.apply("ana", relabel(Kind::Section, "S2", "TAP", "T-OFF"))
        .unwrap();
    let ja = |s: &Session, id: &str| {
        s.line()
            .jumpers
            .iter()
            .find(|j| j.id == id)
            .unwrap()
            .clone()
    };
    assert!(
        matches!(ja(&s, "JA").from.as_tap(), Some(ohl_line::Tap::Span(r)) if r.point == "T-OFF")
    );
    s.apply("ana", relabel(Kind::Jumper, "JA", "K", "K2"))
        .unwrap();
    assert_eq!(ja(&s, "JA").points[0].label, "K2");
    assert!(
        matches!(ja(&s, "JB").from.as_tap(), Some(ohl_line::Tap::Jumper { point, .. }) if point == "K2")
    );
    assert!(
        s.apply("ana", relabel(Kind::Jumper, "JA", "K2", "K2"))
            .is_err()
    );
    s.undo("ana").unwrap();
    assert_eq!(ja(&s, "JA").points[0].label, "K");
}

/// Replacing the line keeps the replacing line's order; spacers are in
/// the spatial index.
#[test]
fn a_replaced_line_keeps_its_order_and_spacers_are_indexed() {
    let (_p, mut s) = project("replace-order");
    let mut line = s.export();
    line.structures.reverse();
    line.sections.reverse();
    s.apply(
        "ana",
        Command::ReplaceLine {
            line: Box::new(line.clone()),
        },
    )
    .unwrap();
    let ids = |l: &Line| {
        l.structures
            .iter()
            .map(|x| x.id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(s.line()), ids(&line));
    assert_eq!(s.line().sections[0].id, line.sections[0].id);
    // A spacer between S1 and S2 in their first span.
    let mut line = s.export();
    for sec in line
        .sections
        .iter_mut()
        .filter(|x| x.id == "S1" || x.id == "S2")
    {
        sec.span_points.push(ohl_line::SpanPoint {
            label: "M".into(),
            span: 0,
            at: ohl_line::SpanPlace::at_fraction(0.5),
            description: String::new(),
            object: Some(ohl_line::LineObject::clamp(0.0)),
        });
    }
    let end = |sec: &str| ohl_line::SpanPointRef {
        section: sec.into(),
        point: "M".into(),
    };
    line.spacers.push(ohl_line::Spacer {
        id: "SP".into(),
        a: end("S1"),
        b: end("S2"),
        weight: 100.0,
        ..Default::default()
    });
    s.apply(
        "ana",
        Command::ReplaceLine {
            line: Box::new(line),
        },
    )
    .unwrap();
    assert!(s.spatial_entry(Kind::Spacer, "SP").is_some());
}

/// The line's settings are rows of their own: two users changing
/// different settings from the same state keep both, and each undoes
/// their own; a project with all settings in one row reads as before.
#[test]
fn settings_changed_by_two_users_are_both_kept() {
    let (_p, mut s) = project("settings-rows");
    let base = s.line().clone();
    let mut a = base.clone();
    a.defaults.jumper_stiffness_factor = 0.5;
    let mut b = base.clone();
    b.solver.jumper_loads = ohl_line::JumperLoads::Pull;
    // Both made on `base` (as two clients would).
    s.apply("ana", line_settings_change(&base, &a)).unwrap();
    s.apply("ben", line_settings_change(&base, &b)).unwrap();
    assert_eq!(s.line().defaults.jumper_stiffness_factor, 0.5);
    assert_eq!(s.line().solver.jumper_loads, ohl_line::JumperLoads::Pull);
    // Ana's undo: her row only, Ben's change stays.
    s.undo("ana").unwrap();
    assert_eq!(
        s.line().defaults.jumper_stiffness_factor,
        base.defaults.jumper_stiffness_factor
    );
    assert_eq!(s.line().solver.jumper_loads, ohl_line::JumperLoads::Pull);
    // All settings in the `line` row (projects written before): read,
    // the rows over it.
    let mut objects = s.objects.clone();
    let mut all = Meta::of(s.line());
    all.name = "old form".into();
    for k in META_ROWS {
        objects.remove(&(Kind::Meta, k.to_owned()));
    }
    objects
        .get_mut(&(Kind::Meta, "line".to_owned()))
        .unwrap()
        .value = json(&all);
    let read = meta_of(&objects).unwrap().unwrap();
    assert_eq!(read.name, "old form");
    assert_eq!(read.solver.jumper_loads, ohl_line::JumperLoads::Pull);
}

/// What the line sends to the placed structures (load cases, weather
/// cases on their own): its own settings row (`structure_loads`), put
/// alone, read back when the project opens again, no section solved
/// again; undone and redone.
#[test]
fn structure_loads_are_a_settings_row() {
    let (path, mut s) = project("structure-loads");
    assert!(s.line().structure_loads.is_default());
    let choice = ohl_line::StructureLoads {
        load_cases: Some(vec!["wind".into()]),
        weather_cases: vec!["ice".into(), "wind".into()],
    };
    let command = s.structure_loads_command(choice.clone());
    let Command::Batch { commands } = &command else {
        panic!("{command:?}")
    };
    assert!(
        matches!(&commands[..], [Command::Put { kind: Kind::Meta, id, .. }] if id == "structure_loads"),
        "{commands:?}"
    );
    let applied = s.apply("ana", command).unwrap();
    assert!(applied.sections.is_empty(), "{:?}", applied.sections);
    assert_eq!(s.line().structure_loads, choice);
    let row = &s.objects[&(Kind::Meta, "structure_loads".to_owned())].value;
    assert_eq!(
        row,
        &serde_json::json!({ "structure_loads": {
            "load_cases": ["wind"], "weather_cases": ["ice", "wind"]
        } })
    );
    drop(s);
    let mut s = Session::open(&path).unwrap();
    assert_eq!(s.line().structure_loads, choice);
    s.undo("ana").unwrap();
    assert!(s.line().structure_loads.is_default());
    s.redo("ana").unwrap();
    assert_eq!(s.line().structure_loads, choice);
}

/// A project still to be upgraded to alignment points keeps its library
/// files when it opens (the upgrade rewrites the line's objects only).
#[test]
fn the_routes_upgrade_keeps_library_files() {
    let (path, mut s) = project("upgrade-libraries");
    s.put_library_file("ana", "cables", "a.ohlw", "x = 1")
        .unwrap();
    drop(s);
    {
        let c = Connection::open(&path).unwrap();
        c.execute("DELETE FROM ols_meta WHERE key = 'alignment_points'", [])
            .unwrap();
    }
    let s = Session::open(&path).unwrap();
    assert_eq!(
        s.library_files("cables").unwrap(),
        vec![("a.ohlw".to_owned(), "x = 1".to_owned())]
    );
}
