// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Joints with a kind and attachment points linked to them (2026-10-21):
//! the joints the points make, their ids, kinds and offsets, and the
//! shipped structure types read with their points where they were.

use std::path::{Path, PathBuf};

use crate::structures::{Detail, JointKind, JointSource, StructureType};
use crate::{LibraryItem, PointPlace, point_joints};

fn read(text: &str) -> StructureType {
    StructureType::parse(Path::new("t.ohls"), text).unwrap()
}

const HEAD: &str = "id = \"T\"\nname = \"t\"\nkind = \"lattice\"\n";

/// A frame of two joints and an arm between them.
const ARM: &str = "[[node]]\nid = \"N1\"\npoint = [\"0 m\", \"0 m\", \"30 m\"]\n\
    support = \"fixed\"\n\
    [[node]]\nid = \"N2\"\npoint = [\"-10 m\", \"0 m\", \"30 m\"]\n\
    [[member]]\nid = \"arm\"\nnodes = [\"N1\", \"N2\"]\n";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The shipped structure types: every point where its file puts it (on
/// its joint exactly, along its member as before), every point linked
/// to a joint, the detail levels as before the joints had kinds.
#[test]
fn shipped_types_keep_their_points() {
    let files = [
        (
            "examples/synthetic-line/Structures/type-01.ohls",
            Detail::Stick,
        ),
        (
            "examples/synthetic-line/Structures/type-02.ohls",
            Detail::Design,
        ),
        (
            "examples/synthetic-line/Structures/type-03.ohls",
            Detail::Stick,
        ),
        (
            "examples/synthetic-line/Structures/type-04.ohls",
            Detail::Stick,
        ),
        // A named pole file: a full design.
        (
            "examples/synthetic-line/Structures/type-05.ohls",
            Detail::Design,
        ),
        (
            "examples/synthetic-line/Structures/type-06.ohls",
            Detail::Stick,
        ),
        (
            "examples/starter/structures/starter-suspension.ohls",
            Detail::Stick,
        ),
        (
            "examples/starter/structures/starter-strain.ohls",
            Detail::Stick,
        ),
        ("examples/exports/every-profile.ohls", Detail::Design),
    ];
    for (file, level) in files {
        let path = repo().join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        let t = StructureType::parse(&path, &text).unwrap();
        assert_eq!(t.detail(), level, "{file}");
        let joints = t.joints();
        for p in &t.points {
            let j = joints
                .iter()
                .find(|j| j.id == p.joint)
                .unwrap_or_else(|| panic!("{file}: point {} not linked", p.label));
            assert_eq!(j.kind, Some(JointKind::Attach), "{file} {}", p.label);
            let at = p.position().map(|v| v.si());
            match (&p.node, &j.source) {
                // On a joint: exactly its position (bit for bit).
                (Some(n), JointSource::Node) => {
                    assert_eq!(&j.id, n);
                    assert_eq!(at, j.position, "{file} {}", p.label);
                }
                (None, JointSource::Member { .. }) => assert_eq!(at, j.position),
                (None, JointSource::Point) => {
                    // Its own joint: the point's position, its label.
                    assert_eq!(at, j.position);
                    assert_eq!(j.id, p.label);
                }
                other => panic!("{file} {}: {other:?}", p.label),
            }
            assert_eq!(t.point_offset(p), [0.0; 3]);
        }
    }
}

/// type-02's points are on joints of other ids (A11 on A2): the link is
/// the joint, the label stays the point's.
#[test]
fn type_02_points_link_to_their_joints() {
    let path = repo().join("examples/synthetic-line/Structures/type-02.ohls");
    let t = StructureType::parse(&path, &std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(t.point_link("A1"), Some(("A1", [0.0; 3])));
    assert_eq!(t.point_link("A11"), Some(("A2", [0.0; 3])));
    // The other joints: supports at the feet, no kind elsewhere.
    let joints = t.joints();
    assert!(
        joints
            .iter()
            .filter(|j| j.kind == Some(JointKind::Support))
            .count()
            >= 4
    );
    assert_eq!(t.attach_joints().len(), 2);
    assert!(joints.iter().all(|j| j.source == JointSource::Node));
}

/// The joints points make: a stick figure's point is its own joint, its
/// label the id (`-a`, `-a2`… when a node has it); points along a member
/// at one place share one joint; a point at a node's position is linked
/// to that node.
#[test]
fn joints_made_by_points() {
    let joints = [
        ("A1", [0.0, 0.0, 10.0], None),
        ("A1-a", [1.0, 0.0, 10.0], None),
        ("N5", [5.0, 0.0, 10.0], Some("C")),
    ];
    let ids = point_joints(
        &joints,
        &[
            ("A1", PointPlace::Free([-3.0, 0.0, 10.0])),
            ("B", PointPlace::Member("arm", 0.25)),
            ("B2", PointPlace::Member("arm", 0.25)),
            ("B3", PointPlace::Member("arm", 0.5)),
            ("C", PointPlace::Free([9.0, 9.0, 9.0])),
            ("D", PointPlace::Free([1.0, 0.0, 10.0 + 1e-7])),
            ("E", PointPlace::Joint("N5")),
        ],
    );
    assert_eq!(ids, ["A1-a2", "B", "B", "B3", "N5", "A1-a", "N5"]);
}

/// An offset puts the point away from its joint; the joint keeps its
/// place. A point on a node, along a member, with offsets; a point at a
/// position of its own may not take one.
#[test]
fn offsets_from_joints() {
    let t = read(&format!(
        "{HEAD}{ARM}\
         [[point]]\nlabel = \"tip\"\nnode = \"N2\"\noffset = [\"0 m\", \"0.4 m\", \"-0.2 m\"]\n\
         [[point]]\nlabel = \"tip2\"\nnode = \"N2\"\n\
         [[point]]\nlabel = \"mid\"\nmember = \"arm\"\nat = 0.5\noffset = [\"0 m\", \"0 m\", \"-1 m\"]\n"
    ));
    assert_eq!(t.point_local("tip", None), Some([-10.0, 0.4, 29.8]));
    assert_eq!(t.point_local("tip2", None), Some([-10.0, 0.0, 30.0]));
    assert_eq!(t.point_local("mid", None), Some([-5.0, 0.0, 29.0]));
    assert_eq!(t.point_link("tip"), Some(("N2", [0.0, 0.4, -0.2])));
    // Two points on one joint.
    assert_eq!(t.point_link("tip2"), Some(("N2", [0.0; 3])));
    let mid = t.joint("mid").unwrap();
    assert_eq!(mid.position, [-5.0, 0.0, 30.0]);
    assert_eq!(
        mid.source,
        JointSource::Member {
            member: "arm".into(),
            at: 0.5
        }
    );
    assert_eq!(t.joint_local("N2", None), Some([-10.0, 0.0, 30.0]));
    // N1 is supported, N2 carries points; both kinds worked out.
    assert_eq!(t.joint("N1").unwrap().kind, Some(JointKind::Support));
    assert_eq!(t.joint("N2").unwrap().kind, Some(JointKind::Attach));
    let free = format!(
        "{HEAD}[[point]]\nlabel = \"x\"\npoint = [\"0 m\", \"0 m\", \"1 m\"]\n\
         offset = [\"0 m\", \"0 m\", \"1 m\"]\n"
    );
    assert!(StructureType::parse(Path::new("t.ohls"), &free).is_err());
}

/// A joint's kind as written: a point may not hang from a support joint;
/// a support joint without a support lacks it for a full design; a
/// joint with no kind and nothing on it has none.
#[test]
fn joint_kinds_as_written() {
    let t = read(&format!(
        "{HEAD}{ARM}[[node]]\nid = \"N3\"\npoint = [\"5 m\", \"0 m\", \"30 m\"]\nkind = \"attach\"\n\
         [[node]]\nid = \"N4\"\npoint = [\"5 m\", \"0 m\", \"0 m\"]\nkind = \"support\"\n\
         [[node]]\nid = \"N5\"\npoint = [\"5 m\", \"0 m\", \"10 m\"]\n"
    ));
    assert_eq!(t.joint("N3").unwrap().kind, Some(JointKind::Attach));
    assert_eq!(t.joint("N4").unwrap().kind, Some(JointKind::Support));
    assert_eq!(t.joint("N5").unwrap().kind, None);
    assert!(
        t.missing_for_design()
            .iter()
            .any(|m| m.contains("support joint N4 without a support"))
    );
    let linkable: Vec<String> = t.linkable_joints().into_iter().map(|j| j.id).collect();
    assert_eq!(linkable, ["N2", "N3", "N5"]);
    let bad = format!(
        "{HEAD}{ARM}[[node]]\nid = \"N4\"\npoint = [\"5 m\", \"0 m\", \"0 m\"]\nkind = \"support\"\n\
         [[point]]\nlabel = \"x\"\nnode = \"N4\"\n"
    );
    assert!(StructureType::parse(Path::new("t.ohls"), &bad).is_err());
    let typo = format!("{HEAD}[[node]]\nid = \"N\"\npoint = [0, 0, 0]\nkind = \"foot\"\n");
    assert!(StructureType::parse(Path::new("t.ohls"), &typo).is_err());
}

/// A point on no joint of the members (its own joint) keeps a type with
/// members from a full design: its load would not reach the members.
#[test]
fn a_point_on_no_member_blocks_a_full_design() {
    let sections = "[[member]]\nid = \"col\"\nnodes = [\"N0\", \"N1\"]\nsection = \"s\"\n\
        [[node]]\nid = \"N0\"\npoint = [\"0 m\", \"0 m\", \"0 m\"]\nsupport = \"fixed\"\n";
    let designed = format!(
        "{HEAD}{}\n[[point]]\nlabel = \"A\"\nnode = \"N2\"\n",
        ARM.replace("[[member]]\nid = \"arm\"\nnodes = [\"N1\", \"N2\"]\n", "")
            + "[[member]]\nid = \"arm\"\nnodes = [\"N1\", \"N2\"]\nsection = \"s\"\n"
            + sections
    );
    assert_eq!(read(&designed).detail(), Detail::Design);
    let off = format!("{designed}[[point]]\nlabel = \"B\"\npoint = [\"3 m\", \"3 m\", \"3 m\"]\n");
    let t = read(&off);
    assert_eq!(t.detail(), Detail::Outline);
    assert!(
        t.missing_for_design()
            .iter()
            .any(|m| m.contains("attachment point B on no joint")),
        "{:?}",
        t.missing_for_design()
    );
    // At a joint's position it is on that joint.
    let at_joint =
        format!("{designed}[[point]]\nlabel = \"B\"\npoint = [\"0 m\", \"0 m\", \"30 m\"]\n");
    let t = read(&at_joint);
    assert_eq!(t.detail(), Detail::Design);
    assert_eq!(t.point_link("B"), Some(("N1", [0.0; 3])));
}
