// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The leg-rotation check ([`crate::leg_roll`]) on invented towers.

use std::collections::BTreeMap;

use crate::joint_layout::{BoltArea, LayoutSettings};
use crate::joints_tests::library;
use crate::{SolveAsk, joint_details, leg_rolls, solve_joint};

/// The K-brace with its leg (both pieces, one leg) turned `deg` about its
/// axis.
fn k_turned(deg: f64) -> String {
    crate::joint_layout_tests::K.replace(
        "orientation = \"x+y+\"\ndesign = { class = \"leg\" }",
        &format!("orientation = \"x+y+\"\nroll = \"{deg} deg\"\ndesign = {{ class = \"leg\" }}"),
    )
}

/// The leg as the faces want it: its joints say nothing; turned 5° against
/// its two diagonals (the owner's 415-like picture): both pieces flagged,
/// the turn that takes it back (about −5°, within a tenth), each with the
/// other in line as one leg; turned by what is proposed, nothing flagged.
#[test]
fn a_leg_turned_against_its_joints_is_found_from_the_joints() {
    let lib = library();
    let bolts = Default::default();
    let d = joint_details(&k_turned(0.0), &lib, &bolts).unwrap();
    assert!(
        leg_rolls(&d).iter().all(|r| !r.flagged()),
        "{:?}",
        leg_rolls(&d)
    );
    let d = joint_details(&k_turned(5.0), &lib, &bolts).unwrap();
    let rolls = leg_rolls(&d);
    for m in ["L1", "L2"] {
        let r = rolls
            .iter()
            .find(|r| r.member == m)
            .unwrap_or_else(|| panic!("{rolls:?}"));
        let t = r.turn.unwrap_or_else(|| panic!("{r:?}"));
        assert!((t + 5.0).abs() < 0.1, "{r:?}");
        assert!(r.partners.iter().any(|p| p != m), "{r:?}");
        assert!(r.words().starts_with(&format!("turn leg piece {m} by")));
    }
    // Within 2°: not flagged (the owner's flatness).
    let d = joint_details(&k_turned(1.5), &lib, &bolts).unwrap();
    assert!(leg_rolls(&d).iter().all(|r| !r.flagged()));
}

/// Solve the clashes proposes the turn (never applies it): the joint's
/// leg pieces turned alike, its solution worked out with it.
#[test]
fn the_leg_turn_is_proposed_in_solve_the_clashes_not_applied() {
    let lib = library();
    let bolts = Default::default();
    let s = LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    };
    let text = k_turned(5.0);
    let sol = solve_joint(
        &text,
        &SolveAsk {
            library: &lib,
            bolts: &bolts,
            settings: &s,
            preset_fields: BTreeMap::new(),
            leg_bolt: None,
            joint: "N2".to_owned(),
        },
    )
    .unwrap();
    let l = sol.leg_turns.first().unwrap_or_else(|| panic!("{sol:?}"));
    assert!(l.members.contains(&"L1".to_owned()) && l.members.contains(&"L2".to_owned()));
    // The solution chosen never turns the leg.
    if let Some(b) = sol.best() {
        assert!(b.text.contains("roll = \"5 deg\""), "the leg kept");
    }
    if let Some(w) = &l.with {
        assert!(!w.text.contains("roll = \"5 deg\""), "the leg turned there");
    }
}

/// O44 L18: beside it the leg's run turned from its joints' planes (the
/// whole leg in line, about −5°), its own tick; each worked out with every
/// joint the turned members reach laid out with it and its effect counted
/// there (both ends of both pieces), never applied unless ticked.
#[test]
fn the_planes_turn_is_offered_beside_it_and_checked_at_every_joint_it_reaches() {
    let lib = library();
    let bolts = Default::default();
    let s = LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    };
    let sol = solve_joint(
        &k_turned(5.0),
        &SolveAsk {
            library: &lib,
            bolts: &bolts,
            settings: &s,
            preset_fields: BTreeMap::new(),
            leg_bolt: None,
            joint: "N2".to_owned(),
        },
    )
    .unwrap();
    let j = sol
        .leg_turns
        .iter()
        .find(|l| l.source == crate::TurnSource::Joints)
        .unwrap_or_else(|| panic!("{:?}", sol.leg_turns));
    let p = sol
        .leg_turns
        .iter()
        .find(|l| l.source == crate::TurnSource::Planes)
        .unwrap_or_else(|| panic!("{:?}", sol.leg_turns));
    assert!((p.turn + 5.0).abs() < 0.1, "{p:?}");
    assert!(p.plane.is_some() && j.roll.is_some());
    assert!(p.members.contains(&"L1".to_owned()) && p.members.contains(&"L2".to_owned()));
    if let Some(r) = &p.reach {
        for n in ["N1", "N2", "N3"] {
            assert!(r.joints.iter().any(|x| x == n), "{r:?}");
        }
        assert!(r.after <= r.before, "{r:?}");
    }
    if let Some(b) = sol.best() {
        assert!(b.text.contains("roll = \"5 deg\""), "the leg kept");
    }
}

/// Lay out joints says it too (the run's leg turns, never applied: the
/// legs keep their roll).
#[test]
fn the_layout_run_says_the_leg_turned_and_keeps_it() {
    let lib = library();
    let bolts = Default::default();
    let s = LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    };
    let r = crate::lay_out(
        &k_turned(5.0),
        &crate::RunAsk {
            library: &lib,
            bolts: &bolts,
            settings: &s,
            preset_fields: BTreeMap::new(),
            joints: Some(["N2".to_owned()].into()),
            replace_hand: false,
        },
    )
    .unwrap();
    assert!(
        r.leg_rolls.iter().any(|x| x.member == "L2" && x.flagged()),
        "{:?}",
        r.leg_rolls
    );
    assert_eq!(r.text.matches("roll = \"5 deg\"").count(), 2);
    let r0 = crate::lay_out(
        &k_turned(0.0),
        &crate::RunAsk {
            library: &lib,
            bolts: &bolts,
            settings: &s,
            preset_fields: BTreeMap::new(),
            joints: Some(["N2".to_owned()].into()),
            replace_hand: false,
        },
    )
    .unwrap();
    assert!(r0.leg_rolls.is_empty());
}
