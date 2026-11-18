// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

use super::*;

fn example() -> Line {
    Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap()
}

/// A working set written out and worked on: what it refers to comes with
/// it and is not changed there; deletions and renames are refused; its
/// commands go in place and the line kept is the line made again.
#[test]
fn a_part_is_worked_on_within_its_working_set() {
    let dir = std::env::temp_dir().join(format!("ohl-part-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut whole = Session::open(&dir.join("whole.ohl")).unwrap();
    whole.import(&example(), "ana").unwrap();
    let wanted: BTreeSet<(Kind, String)> = [
        (Kind::Section, "S3".to_owned()),
        (Kind::Structure, "T6".to_owned()),
    ]
    .into();
    let path = dir.join("part.ohl");
    let summary = whole.write_part(&wanted, &path, "ana").unwrap();
    assert_eq!(summary.wanted, 2);
    // T3 and T7 (S3's other structures).
    assert_eq!(summary.context, 2);
    assert!(whole.write_part(&wanted, &path, "ana").is_err(), "exists");
    let mut part = Session::open(&path).unwrap();
    assert_eq!(part.part(), Some(&wanted));
    assert!(whole.part().is_none());
    assert_eq!(part.line().sections.len(), 1);
    assert_eq!(part.line().structures.len(), 3);
    part.part_role("ana", Role::Admin).unwrap();
    // The working set changes, in place.
    let p = part.line().structure("T6").unwrap().position;
    let applied = part
        .apply(
            "ana",
            Command::MoveStructure {
                id: "T6".into(),
                position: [p[0] + 2.0, p[1], p[2]],
            },
        )
        .unwrap();
    assert_eq!(applied.sections, ["S3"]);
    part.check_kept().unwrap();
    // The context does not.
    let p = part.line().structure("T3").unwrap().position;
    let err = part
        .apply(
            "ana",
            Command::MoveStructure {
                id: "T3".into(),
                position: [p[0] + 2.0, p[1], p[2]],
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("outside the working set"), "{err}");
    // Nor deletions or renames.
    for c in [
        Command::Delete {
            kind: Kind::Section,
            id: "S3".into(),
            confirm: true,
        },
        Command::Rename {
            kind: Kind::Structure,
            id: "T6".into(),
            new_id: "T6x".into(),
        },
    ] {
        let err = part.apply("ana", c).unwrap_err();
        assert!(err.to_string().contains("on the whole line"), "{err}");
    }
    part.undo("ana").unwrap();
    part.check_kept().unwrap();
    assert_eq!(
        part.line().structure("T6").unwrap().position,
        whole.line().structure("T6").unwrap().position
    );
    drop(part);
    let _ = std::fs::remove_dir_all(&dir);
}
