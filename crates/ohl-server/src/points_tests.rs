// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A LiDAR point changed through the server: every replica's point cloud
//! follows, in the server's log order; the undo too.

use std::path::Path;

use ohl_pointcloud::Store;
use ohl_session::points::{CloudPoint, PointChange};
use ohl_session::{Command, Kind};

use crate::server::Server;
use crate::tests::{Client, local, project, temp};
use crate::{Op, ServerMsg};

/// The stored point nearest to (x, y, z) in the project file `path`.
fn point(path: &Path, at: [f64; 3]) -> Option<CloudPoint> {
    let store = Store::open(path).unwrap();
    store
        .find_point("c", at, None, 0.0005)
        .unwrap()
        .map(|(p, i)| CloudPoint::of(&p, Some(i)))
}

#[test]
fn point_changes_reach_every_replica() {
    let dir = temp("points");
    let path = project(&dir);
    // An invented cloud in the project file: 40 m × 2 m of ground.
    let points = (0..80u32).flat_map(|i| {
        (0..4u32).map(move |j| ohl_lidar::LidarPoint {
            x: 1000.0 + f64::from(i) * 0.5,
            y: 2000.0 + f64::from(j) * 0.5,
            z: 400.0,
            class: 2,
            intensity: 0,
            return_number: 1,
            number_of_returns: 1,
        })
    });
    Store::open(&path)
        .unwrap()
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
    let server = Server::start(&path, local("abcd2345")).unwrap();
    let mut alice = Client::join(&server.invitation_for("alice"), "alice", &dir).unwrap();
    let mut bob = Client::join(&server.invitation_for("bob"), "bob", &dir).unwrap();
    let (alice_file, bob_file) = (dir.join("alice.ohl"), dir.join("bob.ohl"));
    let at = [1010.0, 2000.5, 400.0];
    let from = point(&bob_file, at).expect("the replica holds the cloud");
    // Bob raises a point 2 m and makes it class 6.
    let to = CloudPoint {
        xyz: [from.xyz[0], from.xyz[1], from.xyz[2] + 2000],
        class: 6,
        index: None,
        ..from
    };
    let op = Op::Do {
        command: Command::ChangePoints {
            cloud: "c".into(),
            points: vec![PointChange { from, to }],
        },
    };
    assert_eq!(op.targets(), Some(vec![(Kind::PointCloud, "c".into())]));
    assert_eq!(bob.request(op).unwrap(), "change points");
    alice.pump(|m| matches!(m, ServerMsg::Applied { user, .. } if user == "bob"));
    let raised = [at[0], at[1], at[2] + 2.0];
    for file in [&path, &alice_file, &bob_file] {
        let p = point(file, raised).unwrap_or_else(|| panic!("{}", file.display()));
        assert_eq!(p.class, 6);
        assert!(point(file, at).is_none());
    }
    // Logged as bob's, on every replica the same entry.
    let entry = alice.replica.log().unwrap().pop().unwrap();
    assert_eq!(entry.user, "bob");
    assert_eq!(entry.objects, vec![(Kind::PointCloud, "c".to_owned())]);
    // His undo puts it back everywhere.
    bob.request(Op::Undo).unwrap();
    alice.pump(|m| matches!(m, ServerMsg::Applied { user, .. } if user == "bob"));
    for file in [&path, &alice_file, &bob_file] {
        assert_eq!(point(file, at).map(|p| p.class), Some(2));
        assert!(point(file, raised).is_none());
    }
    drop((alice, bob));
    server.stop();
    std::fs::remove_dir_all(dir).ok();
}
