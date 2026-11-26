// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The reason classes of the joints left ([`crate::left_reasons`], O44
//! L30) on the layout's words (invented members) and on invented joints.

use std::collections::{BTreeMap, BTreeSet};

use crate::joint_layout::{BoltArea, LayoutSettings};
use crate::left_reasons::{LeftReason, joints_left, joints_left_of_run, split_row};
use crate::{RunAsk, lay_out};

/// The words the layout writes (as its format strings build them, invented
/// members), each with its class.
const WORDS: &[(&str, LeftReason)] = &[
    (
        "D7: laid out, D7 and a plate on L2 would run 4 mm into each other (left as it was)",
        LeftReason::Clash,
    ),
    (
        "H3: laid out, the head of D2's bolt 1 and the head of a holding bolt of a plate on L1 \
         would run 2 mm into each other (left as it was)",
        LeftReason::Clash,
    ),
    (
        "D4: laid out, L1 (as it is) would then not fit onto it: its bolt 30 mm from L1's heel \
         as that is written would not stay on it",
        LeftReason::NotFitAfter,
    ),
    (
        "D2: laid out, its bolts on a plate on L1 would lie nearer the plate's edge than their \
         distances (left as it was)",
        LeftReason::NotFitAfter,
    ),
    (
        "R5: its bolted leg lies 45° off the flange of D1 it bolts onto and no turn lays it flat \
         at both its ends; a welded plate in its own plane does not fit: its outstanding leg \
         points towards the flange (it would stand on the plate)",
        LeftReason::LegOffFlange,
    ),
    (
        "R6: its axis runs 4° out of the face of D1's flange it bolts onto",
        LeftReason::AxisOutOfFace,
    ),
    (
        "R7: its outstanding leg points towards the flange (it would stand on the plate)",
        LeftReason::LegIntoFlange,
    ),
    (
        "D3: on the inner face of L1's flange it runs towards the angle's other leg (turn it, or \
         lay it out by hand)",
        LeftReason::InnerFace,
    ),
    (
        "D5: its bolts do not all land on L4's flange, and L4 is built up: a plate on it is \
         drawn through the work point",
        LeftReason::BoltsOffFlange,
    ),
    (
        "D6: on a plate on L1, the plate would run into D9 (stopping short of the work point \
         too)",
        LeftReason::PlateRunsInto,
    ),
    (
        "D8: on a plate its bolts need it pulled back 340 mm to pass the flange of L1 (the \
         longest pull-back 300 mm)",
        LeftReason::PullBack,
    ),
    (
        "D9: 320 mm pulled back to clear L1 (the longest 300 mm; no shift up to 30 mm fits)",
        LeftReason::PullBack,
    ),
    (
        "D1 and D2 within 3°: no room on one plate (D2 needs 320 mm pulled back, the longest \
         300 mm; no shift up to 30 mm fits)",
        LeftReason::PullBack,
    ),
    (
        "R2: too short for its bolts (120 mm to half its length)",
        LeftReason::TooShort,
    ),
    (
        "R3: a bolt of a member left as it is that bolts onto it would not stay on it with any \
         cut and move that fits (its bolt 1)",
        LeftReason::PartnerLeft,
    ),
    (
        "H1: its bolts join nothing — its 2 bolt(s) (its member's design data) are drawn at this \
         joint, but it bolts onto nothing here",
        LeftReason::BoltsJoinNothing,
    ),
    (
        "D2: its cut given by hand (20 mm) leaves it on L1 (it needs 45 mm)",
        LeftReason::GivenByHand,
    ),
    (
        "D3: across the gap between L4's angles, a plate between them would be 8 mm thick",
        LeftReason::Form,
    ),
    ("D3: something the layout never says", LeftReason::Other),
];

#[test]
fn the_layouts_words_are_classed() {
    for (w, c) in WORDS {
        assert_eq!(LeftReason::of(w), *c, "{w}");
    }
    // Every class has a key read back, words and a check.
    for c in LeftReason::ALL {
        assert_eq!(LeftReason::of_key(c.key()), Some(c));
        assert!(!c.as_str().is_empty() && !c.check().is_empty());
    }
}

/// A record's row: ends split where a member of the joint starts, a
/// member's own words holding "; " kept whole.
#[test]
fn a_records_row_is_split_by_member() {
    let members: BTreeSet<&str> = ["R5", "D7", "L1"].into_iter().collect();
    let row = "R5: its bolted leg lies 45° off the flange of D1 it bolts onto and no turn lays \
               it flat at both its ends; a welded plate in its own plane does not fit: x; D7: \
               laid out, D7 and L1 would run 4 mm into each other (left as it was)";
    let parts = split_row(row, &members);
    assert_eq!(parts.len(), 2, "{parts:?}");
    assert!(parts[0].ends_with("does not fit: x"));
    assert_eq!(LeftReason::of(&parts[0]), LeftReason::LegOffFlange);
    assert_eq!(LeftReason::of(&parts[1]), LeftReason::Clash);
}

/// On invented joints: every joint a run leaves not whole is listed with
/// its ends' classes, the same from the run and from the file's record,
/// and no reason the layout gives there is unknown.
#[test]
fn the_joints_left_by_a_run_are_listed_alike_from_the_file() {
    let bolts = Default::default();
    let s = LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    };
    for text in [
        crate::solve_tests::ANGLES,
        crate::joint_layout_tests::K,
        crate::bent_plate_tests::OVER,
    ] {
        let lib = if text == crate::bent_plate_tests::OVER {
            crate::bent_plate_tests::library()
        } else {
            crate::joints_tests::library()
        };
        let ask = RunAsk {
            library: &lib,
            bolts: &bolts,
            settings: &s,
            preset_fields: BTreeMap::new(),
            joints: None,
            replace_hand: false,
        };
        let r = lay_out(text, &ask).unwrap();
        let from_run = joints_left_of_run(&r);
        let from_file = joints_left(&r.text).unwrap();
        let classes = |v: &[crate::left_reasons::LeftJoint]| -> BTreeMap<String, Vec<LeftReason>> {
            v.iter()
                .filter(|j| !j.ends.is_empty())
                .map(|j| (j.joint.clone(), j.ends.iter().map(|e| e.reason).collect()))
                .collect()
        };
        assert_eq!(classes(&from_run), classes(&from_file));
        for j in &from_run {
            for e in &j.ends {
                assert_ne!(e.reason, LeftReason::Other, "{}", e.words);
            }
        }
    }
}
