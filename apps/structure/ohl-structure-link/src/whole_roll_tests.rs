// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! One roll for the whole member (J3).

use crate::whole_roll::{EndTurn, RollWay, whole_roll};

fn end(deg: f64, either_leg: bool) -> EndTurn {
    EndTurn {
        turn: deg.to_radians(),
        either_leg,
    }
}

fn deg(x: Option<(f64, RollWay)>) -> Option<(f64, RollWay)> {
    x.map(|(r, w)| ((r.to_degrees() * 100.0).round() / 100.0, w))
}

#[test]
fn the_same_legs_first_as_before() {
    // Both ends want about 30°: the middle, the same legs.
    assert_eq!(
        deg(whole_roll(&[end(31.0, true), end(29.0, true)], 5.0)),
        Some((30.0, RollWay::SameLegs))
    );
    // A half turn apart lays a leg flat alike (taken within a quarter
    // turn of the first end's).
    assert_eq!(
        deg(whole_roll(&[end(88.0, false), end(-89.0, false)], 5.0)),
        Some((89.5, RollWay::SameLegs))
    );
    // One end alone: its own turn.
    assert_eq!(
        deg(whole_roll(&[end(-12.34, false)], 5.0)),
        Some((-12.34, RollWay::SameLegs))
    );
    assert_eq!(whole_roll(&[], 5.0), None);
}

#[test]
fn an_equal_leg_angle_changes_its_leg_bolted_between_its_ends() {
    // One end needing 10°, the other −78°: a quarter turn from 12° — the
    // other leg there.
    let r = deg(whole_roll(&[end(10.0, true), end(-78.0, true)], 5.0));
    assert_eq!(r, Some((11.0, RollWay::LegChanges)));
    // The same ends with the legs fixed (an unequal angle): no roll.
    assert_eq!(
        whole_roll(&[end(10.0, false), end(-78.0, false)], 5.0),
        None
    );
    // One end may change its leg, the other not: the changing end follows.
    let r = deg(whole_roll(&[end(10.0, false), end(-78.0, true)], 5.0));
    assert_eq!(r, Some((11.0, RollWay::LegChanges)));
}

#[test]
fn never_forced_where_no_roll_serves_both_ends() {
    // 40° apart (and 50° the other way round): no leg of either end flat
    // together.
    assert_eq!(whole_roll(&[end(0.0, true), end(40.0, true)], 5.0), None);
    // The ends' turns within the flatness allowed of each other, or not.
    assert_eq!(
        deg(whole_roll(&[end(0.0, false), end(4.9, false)], 5.0)),
        Some((2.45, RollWay::SameLegs))
    );
    assert_eq!(whole_roll(&[end(0.0, false), end(5.1, false)], 5.0), None);
    // A quarter turn more: the same.
    assert_eq!(
        deg(whole_roll(&[end(0.0, true), end(-85.1, true)], 5.0)),
        Some((2.45, RollWay::LegChanges))
    );
    assert_eq!(whole_roll(&[end(0.0, true), end(-84.9, true)], 5.0), None);
}

#[test]
fn deterministic_and_the_smallest_turn() {
    // Either leg at both ends: the turns 45° ± 1 and −45° ± 1 serve both
    // (a quarter turn apart); the smaller turn by magnitude, then the
    // positive one, whatever the order of the ends.
    let a = whole_roll(&[end(44.0, true), end(-44.0, true)], 5.0);
    let b = whole_roll(&[end(-44.0, true), end(44.0, true)], 5.0);
    assert_eq!(deg(a), Some((45.0, RollWay::LegChanges)));
    assert_eq!(a, b);
    // Within a half-open quarter: never more than 90° either way.
    let c = deg(whole_roll(&[end(80.0, true), end(-12.0, true)], 5.0)).unwrap();
    assert!(c.0.abs() <= 90.0, "{c:?}");
    assert_eq!(c, (-11.0, RollWay::LegChanges));
}
