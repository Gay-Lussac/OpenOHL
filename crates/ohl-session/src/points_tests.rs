// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Points of the project's point cloud changed through the session:
//! logged, undone and redone bit for bit, refused to viewers, outside a
//! user's area and when someone changed the points since.

use std::path::{Path, PathBuf};

use ohl_pointcloud::Store;
use ohl_pointcloud::edit::StoredPoint;

use super::*;
use crate::points::{CloudPoint, PointChange};

/// x of the invented cloud's first points.
const X0: f64 = 2_600_000.0;

/// A project with the example line, users ben (designer) and vic (viewer)
/// and an invented cloud "c": a 0.5 m grid of 120 × 2 m, ground (class 2)
/// with every 5th point vegetation 6 m above (class 5).
fn project(name: &str) -> (PathBuf, Session) {
    let dir = std::env::temp_dir().join(format!("ohl-session-{}-pts-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    for f in ["p.ohl", "p.ohl-wal", "p.ohl-shm"] {
        std::fs::remove_file(dir.join(f)).ok();
    }
    let line = Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap();
    let mut s = Session::open(&path).unwrap();
    s.import(&line, "ana").unwrap();
    s.set_user("ana", "ben", Role::Designer).unwrap();
    s.set_user("ana", "vic", Role::Viewer).unwrap();
    let points = (0..240u32).flat_map(|i| {
        (0..4u32).map(move |j| {
            let veg = (i * 4 + j).is_multiple_of(5);
            ohl_lidar::LidarPoint {
                x: X0 + f64::from(i) * 0.5,
                y: 1_200_000.0 + f64::from(j) * 0.5,
                z: if veg { 506.0 } else { 500.0 } + f64::from(i) * 0.01,
                class: if veg { 5 } else { 2 },
                intensity: (i * 4 + j) as u16,
                return_number: 1,
                number_of_returns: 1,
            }
        })
    });
    let mut store = Store::open(&path).unwrap();
    store
        .import_points(
            "c",
            "",
            None,
            64.0,
            0.001,
            points,
            &ohl_core::budget::Budget::new(),
        )
        .unwrap();
    (path, s)
}

/// Every block and level of the cloud as stored (by what they are, not
/// by row id), and its summary.
fn stored(path: &Path) -> (Vec<String>, String) {
    let conn = Connection::open(path).unwrap();
    let blocks = conn
        .prepare(
            "SELECT cx || ',' || cy || ',' || class || ',' || count || ',' || z0 || ',' ||
                    hex(xyz) || ',' || COALESCE(hex(intensity), '-') || ',' ||
                    COALESCE(hex(returns), '-') || ',' ||
                    COALESCE((SELECT group_concat(level || ':' || count || ':' || z1 || ':' ||
                    hex(xyz), ';') FROM pointcloud_lod WHERE block = b.id), '')
             FROM pointcloud_block b ORDER BY cx, cy, class",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<String>, _>>()
        .unwrap();
    let summary = conn
        .query_row(
            "SELECT points || ',' || xmin || ',' || ymin || ',' || zmin || ',' || xmax ||
                    ',' || ymax || ',' || zmax FROM pointcloud",
            [],
            |r| r.get(0),
        )
        .unwrap();
    (blocks, summary)
}

/// The stored point at (x, y) of class `class`.
fn at(path: &Path, x: f64, y: f64, class: u8) -> CloudPoint {
    let store = Store::open(path).unwrap();
    let (p, i) = store
        .find_point("c", [x, y, 503.0], Some(class), 10.0)
        .unwrap()
        .unwrap();
    assert_eq!(
        (p.xyz[0], p.xyz[1]),
        ((x * 1000.0).round() as i64, (y * 1000.0).round() as i64)
    );
    CloudPoint::of(&p, Some(i))
}

/// `from` raised `dz` (mm) and given class `class`.
fn change(from: CloudPoint, dz: i64, class: u8) -> PointChange {
    PointChange {
        from,
        to: CloudPoint {
            xyz: [from.xyz[0], from.xyz[1], from.xyz[2] + dz],
            class,
            index: None,
            ..from
        },
    }
}

fn present(path: &Path, p: &CloudPoint) -> bool {
    let conn = Connection::open(path).unwrap();
    let cloud = ohl_pointcloud::edit::cloud_named(&conn, "c").unwrap();
    ohl_pointcloud::edit::points_present(&conn, &cloud, &[p.stored()]).unwrap()
}

fn cmd(points: Vec<PointChange>) -> Command {
    Command::ChangePoints {
        cloud: "c".into(),
        points,
    }
}

#[test]
fn changed_points_are_logged_and_undone_bit_for_bit() {
    let (path, mut s) = project("undo");
    let before = stored(&path);
    let line = s.export();
    let a = at(&path, X0 + 1.0, 1_200_000.5, 2);
    let b = at(&path, X0 + 70.0, 1_200_001.0, 2);
    // Two points: one raised 1.5 m and made vegetation, one into a class
    // of its own (a new block) — one command, one log entry.
    let ca = change(a, 1500, 5);
    let cb = change(b, -250, 40);
    let applied = s.apply("ben", cmd(vec![ca, cb])).unwrap();
    assert_eq!(applied.objects, vec![(Kind::PointCloud, "c".to_owned())]);
    assert!(applied.sections.is_empty(), "no section to solve again");
    assert!(present(&path, &ca.to) && present(&path, &cb.to));
    assert!(!present(&path, &a) && !present(&path, &b));
    let after = stored(&path);
    assert_ne!(after, before);
    // The log: ben's entry, the command as applied (with the places).
    let log = s.log().unwrap();
    let last = log.last().unwrap();
    assert_eq!((last.user.as_str(), last.kind.as_str()), ("ben", "do"));
    assert_eq!(last.objects, vec![(Kind::PointCloud, "c".to_owned())]);
    let Command::Batch { commands } = &last.command else {
        panic!("a batch");
    };
    assert!(matches!(&commands[0], Command::ChangePoints { points, .. }
        if points.len() == 2
            && points.iter().all(|c| c.from.index.is_some() && c.to.index.is_some())));
    // What changed, for the applications (the undo: the changes back).
    assert_eq!(applied.points.len(), 1);
    assert_eq!(applied.points[0].1[0].to.xyz, ca.to.xyz);
    // Undone: every stored byte as before; redone: as after.
    let undone = s.undo("ben").unwrap();
    assert_eq!(undone.points[0].1[1].to.xyz, a.xyz);
    assert_eq!(stored(&path), before);
    assert!(present(&path, &a) && !present(&path, &ca.to));
    s.redo("ben").unwrap();
    assert_eq!(stored(&path), after);
    s.undo("ben").unwrap();
    assert_eq!(stored(&path), before);
    // The line is untouched by all this.
    assert_eq!(s.export(), line);
    // After the project is opened again too.
    drop(s);
    let s = Session::open(&path).unwrap();
    assert_eq!(
        s.log().unwrap().iter().filter(|e| e.kind == "undo").count(),
        2
    );
}

#[test]
fn point_changes_are_refused_to_viewers_and_when_changed_since() {
    let (path, mut s) = project("refused");
    let a = at(&path, X0 + 2.0, 1_200_001.5, 2);
    let first = change(a, 1000, 5);
    // A viewer may not change points.
    let before = stored(&path);
    assert!(s.apply("vic", cmd(vec![first])).is_err());
    assert_eq!(stored(&path), before);
    // A point that is not there: refused, nothing written, no entry.
    let entries = s.log().unwrap().len();
    let ghost = change(
        CloudPoint {
            xyz: [0, 0, 0],
            ..a
        },
        1,
        2,
    );
    assert!(s.apply("ben", cmd(vec![ghost])).is_err());
    assert_eq!(
        (stored(&path), s.log().unwrap().len()),
        (before.clone(), entries)
    );
    // Unchanged, or no point: refused.
    assert!(s.apply("ben", cmd(vec![change(a, 0, a.class)])).is_err());
    assert!(s.apply("ben", cmd(vec![])).is_err());
    // ben changes it, ana changes it again: ben's undo is refused (his
    // point is not there any more) until ana undoes hers.
    s.apply("ben", cmd(vec![first])).unwrap();
    let now = CloudPoint {
        index: None,
        ..first.to
    };
    s.apply("ana", cmd(vec![change(now, 200, 6)])).unwrap();
    let err = s.undo("ben").unwrap_err();
    assert!(err.to_string().contains("changed after entry"), "{err}");
    s.undo("ana").unwrap();
    s.undo("ben").unwrap();
    assert_eq!(stored(&path), before);
    // Put and delete do not take a point cloud.
    assert!(
        s.apply(
            "ben",
            Command::Put {
                kind: Kind::PointCloud,
                id: "c".into(),
                value: serde_json::json!({}),
                ord: None
            }
        )
        .is_err()
    );
}

#[test]
fn point_changes_follow_the_area_granted_and_admin_rollback() {
    let (path, mut s) = project("grant");
    let before = stored(&path);
    // ben may edit only west of X0 + 30 m.
    s.grant(
        "ana",
        &collab::Grant {
            subject: "ben".into(),
            kinds: vec![],
            objects: vec![],
            area: Some(vec![
                [X0 - 100.0, 1_199_000.0],
                [X0 + 30.0, 1_199_000.0],
                [X0 + 30.0, 1_201_000.0],
                [X0 - 100.0, 1_201_000.0],
            ]),
            note: String::new(),
        },
    )
    .unwrap();
    let inside = at(&path, X0 + 10.0, 1_200_000.5, 2);
    let outside = at(&path, X0 + 50.0, 1_200_000.5, 2);
    let err = s
        .apply("ben", cmd(vec![change(outside, 300, 2)]))
        .unwrap_err();
    assert!(err.to_string().contains("may edit only"), "{err}");
    // Moved out of the area: refused too.
    let mut far = change(inside, 0, 2);
    far.to.xyz[0] += 40_000;
    assert!(s.apply("ben", cmd(vec![far])).is_err());
    let seq = s.log().unwrap().last().unwrap().seq;
    s.apply("ben", cmd(vec![change(inside, 300, 5)])).unwrap();
    s.apply("ana", cmd(vec![change(outside, -300, 9)])).unwrap();
    assert_ne!(stored(&path), before);
    // The admin brings the project back to before both.
    s.rollback_after("ana", seq).unwrap().unwrap();
    assert_eq!(stored(&path), before);
}

/// The stored point as the command names it, and the command's JSON.
#[test]
fn a_cloud_point_converts_both_ways() {
    let p = StoredPoint {
        xyz: [1, -2, 3],
        class: 7,
        intensity: 9,
        returns: 0x12,
    };
    assert_eq!(CloudPoint::of(&p, Some(4)).stored(), p);
    let json = serde_json::to_value(cmd(vec![PointChange {
        from: CloudPoint::of(&p, None),
        to: CloudPoint::of(&p, Some(2)),
    }]))
    .unwrap();
    assert_eq!(json["op"], "change_points");
    assert!(json["points"][0]["from"].get("index").is_none());
    assert_eq!(json["points"][0]["to"]["index"], 2);
}
