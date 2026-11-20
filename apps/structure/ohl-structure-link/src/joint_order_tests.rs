// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Bottom up, then out (L1 a): the body, the arms and the top from the
//! geometry, and the order.

use std::fmt::Write as _;

use ohl_structures::{LibraryItem, StructureType};

use crate::joint_order::{Zone, joint_order};

/// An invented tower: a tapering body of three levels (legs of class
/// `leg`), an arm at level 2 running out from J20 and J23 (its chords of
/// class `leg`, lying flat), a peak above.
fn tower() -> StructureType {
    let mut s =
        crate::orient_tests::body(&[(0.0, 2.0, 2.0), (6.0, 1.6, 1.6), (12.0, 1.2, 1.2)], &[]);
    for (id, p) in [
        ("A1", [3.2, 1.2, 12.0]),
        ("A2", [3.2, -1.2, 12.0]),
        ("A3", [5.0, 0.0, 12.0]),
        ("T1", [0.0, 0.0, 16.0]),
    ] {
        let _ = write!(
            s,
            "\n[[node]]\nid = \"{id}\"\npoint = [\"{} m\", \"{} m\", \"{} m\"]\n",
            p[0], p[1], p[2]
        );
    }
    for (id, a, b, class) in [
        ("C1", "J20", "A1", "leg"),
        ("C2", "J23", "A2", "leg"),
        ("C3", "A1", "A3", "leg"),
        ("C4", "A2", "A3", "leg"),
        ("C5", "A1", "A2", "bracing"),
        ("T10", "J20", "T1", "leg"),
        ("T11", "J21", "T1", "leg"),
        ("T12", "J22", "T1", "leg"),
        ("T13", "J23", "T1", "leg"),
    ] {
        let _ = write!(
            s,
            "\n[[member]]\nid = \"{id}\"\nnodes = [\"{a}\", \"{b}\"]\nsection = \"EA-61x7\"\n\
             kind = \"frame\"\ndesign = {{ class = \"{class}\" }}\n"
        );
    }
    StructureType::parse(std::path::Path::new("t.ohls"), &s).unwrap()
}

#[test]
fn the_body_the_arms_and_the_top_from_the_legs_lines() {
    let t = tower();
    let o = joint_order(&t);
    for j in ["J00", "J13", "J21", "J22"] {
        assert_eq!(o.zone_of(j), Zone::Body, "{j}");
    }
    for j in ["A1", "A2", "A3"] {
        assert_eq!(o.zone_of(j), Zone::Arm, "{j}");
    }
    // The peak above the highest arm joint (its lines are legs, steep
    // enough: the outline there is the peak's point).
    assert_eq!(o.zone_of("T1"), Zone::Top);
}

#[test]
fn bottom_up_then_out() {
    let t = tower();
    let o = joint_order(&t);
    let order: Vec<&str> = {
        let mut v: Vec<(&usize, &String)> = o.rank.iter().map(|(j, r)| (r, j)).collect();
        v.sort();
        v.into_iter().map(|(_, j)| j.as_str()).collect()
    };
    // The body's twelve joints first, level by level from the bottom.
    let level = |j: &str| j.chars().nth(1).and_then(|c| c.to_digit(10));
    let body: Vec<Option<u32>> = order[..12].iter().map(|j| level(j)).collect();
    assert_eq!(body, [[Some(0); 4], [Some(1); 4], [Some(2); 4]].concat());
    // Then outwards: the arm's near joints (2 m out), its tip (3.8 m out)
    // and the peak (4 m above the arm) by how far out.
    assert_eq!(&order[12..], &["A1", "A2", "A3", "T1"]);
    // The same again (deterministic).
    assert_eq!(joint_order(&t), o);
}

#[test]
fn no_legs_every_joint_of_the_body_bottom_up() {
    let text = crate::orient_tests::body(&[(0.0, 2.0, 2.0), (6.0, 1.6, 1.6)], &[])
        .replace("class = \"leg\"", "class = \"bracing\"");
    let t = StructureType::parse(std::path::Path::new("t.ohls"), &text).unwrap();
    let o = joint_order(&t);
    assert!(o.zone.values().all(|z| *z == Zone::Body));
    assert!(o.rank_of("J03") < o.rank_of("J10"));
}
