// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **One roll for the whole member** (the automatic joint layout's J3,
//! owner 2026-11-19: "each angle's turn chosen for the whole member: a
//! roll that lets both its ends lie flat, the leg bolted allowed to change
//! between its ends where that is how towers are built"; the same day "you
//! can do 90 degrees turn or more if necessary").
//!
//! An angle's roll is one value for the member, so its two ends cannot be
//! laid flat each on its own: the turn that lays one end's leg bolted flat
//! on the flange it bolts onto must also lay the other's. Each end gives
//! the turns that lay it flat:
//!
//! - the leg bolted there (the long or short one given, or the one lying
//!   flattest): that turn and a half turn more (the same leg on the
//!   flange, its outstanding leg the other way);
//! - **an equal-leg angle** whose leg bolted is not given as both legs: a
//!   quarter turn more as well — its *other* leg then lies on the flange.
//!   Both legs of an equal-leg angle are alike for its checks (the one leg
//!   bolted, its eccentricity gauge to centroid), so the leg bolted may
//!   change between its ends, as it does on a tower where a member runs
//!   from a horizontal flange to an upright one (a hip or plan member, a
//!   crossarm's bracing). An unequal angle's leg bolted is its design data
//!   (the long or short leg in its check): never changed by a quarter
//!   turn here.
//!
//! The member's turn is the middle of a turn of every end, where those lie
//! within [`crate::FLAT_OFF`] of each other (each end then within half of
//! it of flat, as before J3): the same legs at both ends first, so a member
//! that was laid flat that way keeps its turn; the leg changing only where
//! that alone serves both ends; of several such turns the smallest — any
//! turn, a quarter turn or more where needed. None: no one roll lays every
//! end flat — the member is not turned (never forced) and the ends left
//! off their flange go to the layout's step 4 or a hand detail, with the
//! reason.
//!
//! Tried and left out (measured on the private real towers, 2026-11-19):
//! taking a turn that leaves every end within the whole of
//! [`crate::FLAT_OFF`] of flat (the ends' turns up to twice it apart) laid
//! out slightly fewer member ends, not more.
//!
//! Pure functions of the ends' turns: deterministic (the same ends give
//! the same turn), the planner's [`crate::layout_run`] run and its relaxed
//! form for the estimated eccentricities (O45) take the same.

use std::f64::consts::{FRAC_PI_2, PI};

/// One end of a member: the turn that lays its leg bolted flat there
/// (radians, to add to the member's roll; any of it and a half turn more),
/// and whether its other leg may be bolted there instead (an equal-leg
/// angle, its leg bolted not given as both: a quarter turn more lays that
/// one flat).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EndTurn {
    /// The turn laying its leg bolted flat, radians.
    pub turn: f64,
    /// Its other leg may be the one bolted (equal legs).
    pub either_leg: bool,
}

/// How the member's turn was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollWay {
    /// The same leg bolted at every end (as before J3).
    SameLegs,
    /// The leg bolted changes between its ends (an equal-leg angle).
    LegChanges,
}

/// The member's turn laying every end of `ends` flat — a turn of each
/// within `flat_off` degrees of the others, the middle of them rounded to
/// a hundredth of a degree as written — and how; none: no one roll does.
/// The first end is the reference of the turn's half turn (as before J3:
/// the turns of the others taken within a quarter turn of it).
pub fn whole_roll(ends: &[EndTurn], flat_off: f64) -> Option<(f64, RollWay)> {
    let first = ends.first()?;
    // As before J3: every turn within a quarter turn of the first end's
    // (a turn and a half turn more lay a leg flat alike).
    let same: Vec<f64> = ends
        .iter()
        .map(|e| e.turn - PI * ((e.turn - first.turn) / PI).round())
        .collect();
    if let Some(mid) = middle(&same, flat_off) {
        return Some((mid, RollWay::SameLegs));
    }
    if !ends.iter().any(|e| e.either_leg) {
        return None;
    }
    // The leg bolted may change: each end's turns a quarter turn apart
    // where either leg may be bolted. From each end's turns, every end's
    // nearest above it within the window; the smallest turn of those that
    // serve every end.
    let reps = |e: &EndTurn| -> Vec<f64> {
        if e.either_leg {
            vec![e.turn, e.turn + FRAC_PI_2]
        } else {
            vec![e.turn]
        }
    };
    let mut best: Option<f64> = None;
    for anchor in ends.iter().flat_map(reps) {
        let mut picked = Vec::with_capacity(ends.len());
        for e in ends {
            let period = if e.either_leg { FRAC_PI_2 } else { PI };
            let d = (e.turn - anchor).rem_euclid(period);
            if d.to_degrees() > flat_off + 1e-9 {
                break;
            }
            picked.push(anchor + d);
        }
        if picked.len() != ends.len() {
            continue;
        }
        let Some(mid) = middle(&picked, flat_off) else {
            continue;
        };
        // The smallest turn of it (mid and a half turn more lay every end
        // flat alike).
        let mid = to_half_open(mid);
        let better = match best {
            None => true,
            Some(b) => {
                let (x, y) = (round_cd(mid.abs()), round_cd(b.abs()));
                x < y || (x == y && mid > b)
            }
        };
        if better {
            best = Some(mid);
        }
    }
    best.map(|b| (round_deg(b), RollWay::LegChanges))
}

/// The middle of `turns` (radians) when they lie within `flat_off`
/// degrees of each other, rounded to a hundredth of a degree.
fn middle(turns: &[f64], flat_off: f64) -> Option<f64> {
    let lo = turns.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = turns.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    ((hi - lo).to_degrees() <= flat_off + 1e-9).then(|| round_deg((lo + hi) / 2.0))
}

/// `x` (radians) within (−90°, 90°] by half turns.
fn to_half_open(mut x: f64) -> f64 {
    while x > FRAC_PI_2 + 1e-12 {
        x -= PI;
    }
    while x <= -FRAC_PI_2 + 1e-12 {
        x += PI;
    }
    x
}

/// Rounded to a hundredth of a degree, radians.
fn round_deg(x: f64) -> f64 {
    ((x.to_degrees() * 100.0).round() / 100.0).to_radians()
}

/// In hundredths of a degree (for ties).
fn round_cd(x: f64) -> i64 {
    (x.to_degrees() * 100.0).round() as i64
}
