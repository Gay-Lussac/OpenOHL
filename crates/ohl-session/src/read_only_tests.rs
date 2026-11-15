// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! [`Session::open_read_only`] (owner 2026-11-15): commands that only
//! compute leave the project file as it is — its bytes, its modification
//! time, no `-wal` or `-shm` beside it.

use super::*;
use std::path::PathBuf;

fn example() -> Line {
    Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap()
}

/// A closed project file holding the example line, in a folder of its own.
fn closed_project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ohl-session-ro-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p ro#1.ohl");
    let mut s = Session::open(&path).unwrap();
    s.import(&example(), "ana").unwrap();
    drop(s);
    path
}

fn sidecars(path: &Path) -> Vec<String> {
    ["-wal", "-shm", "-journal"]
        .iter()
        .filter(|s| {
            let mut p = path.as_os_str().to_owned();
            p.push(s);
            Path::new(&p).exists()
        })
        .map(|s| (*s).to_owned())
        .collect()
}

#[test]
fn reading_leaves_the_file_unchanged() {
    let path = closed_project("unchanged");
    assert!(sidecars(&path).is_empty(), "{:?}", sidecars(&path));
    let bytes = std::fs::read(&path).unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    {
        let s = Session::open_read_only(&path).unwrap();
        assert_eq!(s.export(), example());
        assert!(s.has_line());
        // While it is open: nothing beside the file either.
        assert!(sidecars(&path).is_empty(), "{:?}", sidecars(&path));
    }
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
    assert!(sidecars(&path).is_empty(), "{:?}", sidecars(&path));
}

#[test]
fn a_command_that_writes_is_refused() {
    let path = closed_project("refused");
    let bytes = std::fs::read(&path).unwrap();
    let mut s = Session::open_read_only(&path).unwrap();
    let p = s.line().structure("T6").unwrap().position;
    let moved = s.apply(
        "ana",
        Command::MoveStructure {
            id: "T6".into(),
            position: [p[0] + 5.0, p[1], p[2]],
        },
    );
    let e = moved.expect_err("written on a read-only session");
    assert!(e.to_string().contains("readonly"), "{e}");
    drop(s);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn a_file_to_upgrade_is_read_through_a_copy_in_memory() {
    let path = closed_project("upgrade");
    // As an older build left it: no head of the version control, which
    // opening writes.
    {
        let c = Connection::open(&path).unwrap();
        c.execute("DELETE FROM ols_meta WHERE key = 'vc_head'", [])
            .unwrap();
        c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    }
    assert!(sidecars(&path).is_empty(), "{:?}", sidecars(&path));
    let bytes = std::fs::read(&path).unwrap();
    let s = Session::open_read_only(&path).unwrap();
    assert_eq!(s.export(), example());
    drop(s);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(sidecars(&path).is_empty(), "{:?}", sidecars(&path));
    // The writing open upgrades it as before.
    drop(Session::open(&path).unwrap());
    let c = Connection::open(&path).unwrap();
    let head: String = c
        .query_row(
            "SELECT value FROM ols_meta WHERE key = 'vc_head'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(head, "branch:main");
}

#[test]
fn a_file_open_elsewhere_is_read_beside_its_writer() {
    let path = closed_project("beside");
    // The desktop application has it open (its write-ahead log beside it)
    // and has just written.
    let mut writer = Session::open(&path).unwrap();
    let p = writer.line().structure("T6").unwrap().position;
    writer
        .apply(
            "ana",
            Command::MoveStructure {
                id: "T6".into(),
                position: [p[0] + 5.0, p[1], p[2]],
            },
        )
        .unwrap();
    let s = Session::open_read_only(&path).unwrap();
    assert_eq!(s.line().structure("T6").unwrap().position[0], p[0] + 5.0);
}

#[test]
fn a_missing_file_is_not_made() {
    let dir = std::env::temp_dir().join(format!("ohl-session-ro-{}-missing", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("none.ohl");
    std::fs::remove_file(&path).ok();
    let e = Session::open_read_only(&path).expect_err("no file");
    assert!(e.to_string().contains("no such project file"), "{e}");
    assert!(!path.exists());
}
