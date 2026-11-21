// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! O44 L11 on invented joints: both faces of one plate, one plate per
//! plane, an end moved to pass the flange on a plate.

use std::collections::BTreeMap;

use crate::bent_plate_tests::{OVER, library};
use crate::joint_layout::{BoltArea, LayoutSettings, Onto};
use crate::layout_run::{RunAsk, lay_out};

fn settings() -> LayoutSettings {
    LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    }
}

fn run(text: &str, s: &LayoutSettings) -> crate::LayoutRun {
    let lib = library();
    let bolts = Default::default();
    lay_out(
        text,
        &RunAsk {
            library: &lib,
            bolts: &bolts,
            settings: s,
            preset_fields: BTreeMap::new(),
            joints: Some(["N2".to_owned()].into()),
            replace_hand: false,
        },
    )
    .unwrap()
}

/// The bent plate's joint (a plan member P1 on the inner face of the
/// horizontal's flat leg, leaving over the heel) with a second plan member
/// P2 arriving at 30° on that leg's outer face from the same side, three
/// bolts: P2 goes onto a flat plate on the leg reaching over the heel.
pub(crate) fn two_sides() -> String {
    OVER.to_owned()
        + r#"
[[node]]
id = "N5"
point = ["4.155 m", "-2 m", "3 m"]
support = "fixed"

[[member]]
id = "P2"
nodes = ["N5", "N2"]
section = "EA-61x7"
design = { class = "redundant", bolts = { diameter = "16 mm", count = 3 } }
"#
}

/// Two plan members on either face of one plate (the owner's picture: "both
/// diagonals could be on the same plate just bolted on different sides"):
/// without, P1 (inner side) on a bent plate round the horizontal's other
/// leg and P2 on a flat plate (three plates); with, both on that flat
/// plate — P1 on its back, past the heel — one plate; written, nothing runs
/// into anything, and the two members' leg faces lie the plate's thickness
/// apart (P1 drawn against the plate's back).
#[test]
fn two_members_on_both_faces_of_one_plate() {
    let s0 = LayoutSettings {
        both_faces: false,
        ..settings()
    };
    let r0 = run(&two_sides(), &s0);
    let n0 = r0.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(n0.written && n0.hand.is_empty(), "{:?}", n0.hand);
    assert_eq!(n0.plan.plates.len(), 3, "{:?}", n0.plan.plates);
    let r = run(&two_sides(), &settings());
    let n2 = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(n2.written && n2.hand.is_empty(), "{:?}", n2.hand);
    assert_eq!(n2.plan.plates.len(), 1, "{:?}", n2.plan.plates);
    assert!(n2.plan.ends.iter().all(|e| e.onto == Onto::Plate(0)));
    assert!(
        n2.plan
            .notes
            .iter()
            .any(|n| n.contains("both faces of one plate")),
        "{:?}",
        n2.plan.notes
    );
    assert_eq!(r.clashes.1, 0, "{:?}", r.clashes);
    let d = crate::joint_details(&r.text, &library(), &Default::default()).unwrap();
    let j = d.joint("N2").unwrap();
    assert!(j.clashes.is_empty(), "{:?}", j.clashes);
    let plate = &j.plates[0];
    let mut members = plate.members.clone();
    members.sort();
    // (H2 by the plate's own bolts onto it.)
    assert_eq!(members, ["H2", "P1", "P2"]);
    let n = plate.through.unwrap();
    let at = |m: &str| {
        let a = j.arrivals.iter().find(|a| a.member == m).unwrap();
        let q = a.bolt_points[0];
        q[0] * n[0] + q[1] * n[1] + q[2] * n[2]
    };
    assert!(
        ((at("P1") - at("P2")).abs() - plate.thickness).abs() < 1e-6,
        "{} {} {}",
        at("P1"),
        at("P2"),
        plate.thickness
    );
}

/// A leg joint like the owner's splice (2026-11-21: "the diagonals need a
/// horizontal shift and a pull back as well as a plate, then one plate for
/// each plane"): a wide leg, a K of diagonals and a horizontal in one plane,
/// the longest pull-back 250 mm. Without the step a diagonal on the plate
/// would need more pull-back to pass the flange: an end left,
/// clashes after. With it, that end moved sideways (its shift, within the
/// shift limit) and pulled back: all three on one plate, nothing left,
/// nothing running into anything; deterministic.
#[test]
fn an_end_moves_sideways_to_pass_the_flange_on_one_plate() {
    let lib = crate::other_face_tests::lib_wide();
    let text = crate::other_face_tests::face_joint(1.5, 2, true);
    let with = |on: bool| {
        let s = LayoutSettings {
            plate_shift: on,
            other_face: false,
            pull_back: 0.25,
            ..settings()
        };
        let bolts = Default::default();
        lay_out(
            &text,
            &RunAsk {
                library: &lib,
                bolts: &bolts,
                settings: &s,
                preset_fields: BTreeMap::new(),
                joints: Some(["N2".to_owned()].into()),
                replace_hand: false,
            },
        )
        .unwrap()
    };
    let r0 = with(false);
    let n0 = r0.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(!n0.hand.is_empty(), "{:?}", n0.plan);
    assert!(n0.plan.ends.len() < 3, "{:?}", n0.plan.ends);
    let r = with(true);
    let n2 = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(n2.written && n2.hand.is_empty(), "{:?}", n2.hand);
    assert_eq!(n2.plan.ends.len(), 3);
    assert_eq!(n2.plan.plates.len(), 1, "{:?}", n2.plan.plates);
    assert_eq!(r.clashes.1, 0);
    let moved: Vec<&crate::EndPlan> = n2
        .plan
        .ends
        .iter()
        .filter(|e| e.shift.is_some_and(|x| x.abs() > 1e-4))
        .collect();
    assert!(!moved.is_empty(), "{:?}", n2.plan.ends);
    for e in &moved {
        assert!(e.shift.unwrap().abs() <= settings().shift_limit + 1e-9);
        assert!(e.cut.unwrap() <= 0.25 + 1e-9);
    }
    assert!(
        n2.plan
            .notes
            .iter()
            .any(|x| x.contains("within the longest pull-back")),
        "{:?}",
        n2.plan.notes
    );
    assert_eq!(with(true).text, r.text);
}
