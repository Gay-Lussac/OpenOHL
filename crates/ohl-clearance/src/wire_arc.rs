// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Wire to wire on the swing arc** (owner, 2026-10-28): the line's own
//! wires checked against each other while the same wind swings them all
//! together from still air to the full blowout of a side, every pressure
//! between included (theory: Clearances › Wire to wire on the swing arc).
//!
//! Each span (and each jumper with a band) swings as one about its
//! attachment axis, as on the ground windows' swing arc ([`SwingArc`](crate::positions::SwingArc),
//! [`crate::zone_arc::zone_arcs`]), but **not independently**: one wind
//! pressure for all, a fraction f of the case's from 0 to 1, and at f
//! every span at its own angle (tan φ grows linearly with the pressure
//! from its still-air angle to its full blowout's; [`SwingArc::swung`](crate::positions::SwingArc::swung)).
//! The check samples f in steps so small that no point of a wire moves
//! more than [`WIRE_ARC_STEP`] between two of them
//! ([`SwingArc::swing_reach`](crate::positions::SwingArc::swing_reach): the angle moves fastest near still air), and each wire position is
//! checked as a solved one ([`crate::case::check_case`]): the rules, their
//! measures and exclusions as in any case.
//!
//! **Coarse, then fine.** The whole line is swung first in coarse steps
//! ([`WIRE_ARC_COARSE`], each wire taken thicker by half of it: a pair
//! whose coarse clearance meets a rule meets it everywhere on the swing);
//! only the pairs that may not are then swung alone in fine steps — the
//! same result at a fraction of the work on a long line.
//!
//! **Conservative:** each swung wire is taken thicker by half a step
//! (between two samples it is never farther than that from one of them)
//! and by its band's margin (the intermediate pressures' wires lie up to
//! that off the rigid swing, as for the ground; a jumper's margin, 0 by
//! default, is the user's responsibility).

use ohl_weather::WindSide;

use crate::checks::Wire;
use crate::zone_arc::{ArcCase, ZoneArc};

/// The most a wire moves between two samples of the swing, m.
pub const WIRE_ARC_STEP: f64 = 0.1;

/// The coarse pass's step, m.
pub const WIRE_ARC_COARSE: f64 = 1.0;

/// The most samples of one side's swing (a long swing is sampled
/// coarser, the wires taken that much thicker).
pub const MAX_SAMPLES: usize = 2000;

/// One side's swing of a windy case, all its wires together.
#[derive(Debug, Clone, PartialEq)]
pub struct Swing {
    /// The case ([`ArcCase`], index).
    pub case: usize,
    /// The side the wind blows from.
    pub side: WindSide,
    /// The full blowout's state (index in the wind states).
    pub state: usize,
    /// The arcs swung (indices in the arcs).
    pub arcs: Vec<usize>,
    /// Samples of the swing (f = 0, 1/n, …, 1).
    pub samples: usize,
    /// How far a wire may lie from the nearest sample, m (half the most
    /// any point moves between two samples).
    pub half_step: f64,
}

/// The samples of a swing of `of` (arc indices) to band side `index`
/// in steps of at most `step` m, and half the most a point moves between
/// two.
fn sampled(arcs: &[ZoneArc], of: &[usize], index: usize, step: f64) -> (usize, f64) {
    // How far a point may move per unit of f, at its fastest.
    let longest = of
        .iter()
        .map(|&k| arcs[k].arc.swing_reach(index))
        .fold(0.0, f64::max);
    let samples = ((longest / step).ceil() as usize).clamp(1, MAX_SAMPLES);
    (samples, longest / samples as f64 / 2.0)
}

/// The swings of `cases` over `arcs` ([`crate::zone_arc::zone_arcs`]) in
/// steps of at most `step` m: per case, one per side solved at its full
/// pressure.
pub fn swings(arcs: &[ZoneArc], cases: &[ArcCase], step: f64) -> Vec<Swing> {
    let mut out = Vec::new();
    for (c, case) in cases.iter().enumerate() {
        let of: Vec<usize> = (0..arcs.len()).filter(|&k| arcs[k].case == c).collect();
        if of.is_empty() {
            continue;
        }
        // Blown to the left: the wind from the right (index 0 of a band).
        for (state, side, index) in [
            (case.left, WindSide::Right, 0),
            (case.right, WindSide::Left, 2),
        ] {
            let Some(state) = state else { continue };
            let (samples, half_step) = sampled(arcs, &of, index, step);
            out.push(Swing {
                case: c,
                side,
                state,
                arcs: of.clone(),
                samples,
                half_step,
            });
        }
    }
    out
}

impl Swing {
    /// The band index of its side (0: blown to the left, 2: right).
    fn index(&self) -> usize {
        if self.side == WindSide::Right { 0 } else { 2 }
    }

    /// The same swing of the arcs `keep` only, in steps of at most `step`
    /// m (a pair swung alone, finer).
    pub fn only(&self, arcs: &[ZoneArc], keep: &[usize], step: f64) -> Swing {
        let (samples, half_step) = sampled(arcs, keep, self.index(), step);
        Swing {
            arcs: keep.to_vec(),
            samples,
            half_step,
            ..self.clone()
        }
    }

    /// The arc of wire `w` (one of [`Swing::wires`]: by its section and
    /// span).
    pub fn arc_of(&self, arcs: &[ZoneArc], w: &Wire) -> Option<usize> {
        self.arcs
            .iter()
            .copied()
            .find(|&k| arcs[k].wire.section == w.section && arcs[k].wire.span == w.span)
    }

    /// The wires at sample `k` (0 ..= samples): each swung that far,
    /// thicker by half a step and its band's margin.
    pub fn wires(&self, arcs: &[ZoneArc], k: usize) -> Vec<Wire> {
        let f = k as f64 / self.samples as f64;
        self.arcs
            .iter()
            .map(|&a| {
                let arc = &arcs[a];
                Wire {
                    points: arc.arc.swung(self.index(), f),
                    polyline_error: arc.wire.polyline_error + self.half_step + arc.arc.margin(),
                    ..arc.wire.clone()
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::positions::{Axis, SwingArc, WireShape};

    fn wire(points: Vec<[f64; 3]>) -> Wire {
        Wire {
            section: "s".into(),
            span: 0,
            circuit: "c".into(),
            phase: "L1".into(),
            potential: None,
            bundle: Vec::new(),
            points,
            ends: Vec::new(),
            surface_radius: 0.0,
            polyline_error: 0.0,
        }
    }

    /// A span hanging 10 m below its axis, blown to 45° each side.
    fn arc(y: f64) -> ZoneArc {
        let axis = Axis::new([0.0, y, 0.0], [100.0, y, 0.0]).unwrap();
        let at = |a: f64| {
            let w = wire(
                (0..=10)
                    .map(|i| [i as f64 * 10.0, y + 10.0 * a.sin(), -10.0 * a.cos()])
                    .collect(),
            );
            WireShape::new(w, &axis)
        };
        let q = std::f64::consts::FRAC_PI_4;
        let (l, m, r) = (at(-q), at(0.0), at(q));
        let band = SwingArc::with_margin(axis, &l, &m, &r, 0.0).unwrap();
        ZoneArc {
            case: 0,
            state: 0,
            wire: m.wire.clone(),
            radius: band.extent().0,
            arc: band,
        }
    }

    /// The swing passes through still air and the full blowout, at the
    /// angle whose tangent is the fraction of the full one's, the radius
    /// kept; samples close enough that no wire moves more than a step.
    #[test]
    fn wires_swing_together_by_the_tangent_of_the_angle() {
        let arcs = vec![arc(0.0), arc(5.0)];
        let cases = vec![ArcCase {
            name: "wind".into(),
            still: 0,
            left: Some(1),
            right: Some(2),
        }];
        let all = swings(&arcs, &cases, WIRE_ARC_STEP);
        assert_eq!(all.len(), 2);
        let right = all.iter().find(|s| s.side == WindSide::Left).unwrap();
        assert_eq!(right.state, 2);
        assert!(right.half_step <= WIRE_ARC_STEP / 2.0 + 1e-12);
        let first = right.wires(&arcs, 0);
        assert!((first[0].points[5][2] + 10.0).abs() < 1e-9);
        let last = right.wires(&arcs, right.samples);
        let p = last[0].points[5];
        assert!(
            (p[1] - 10.0 * std::f64::consts::FRAC_PI_4.sin()).abs() < 1e-9,
            "{p:?}"
        );
        let half = right.wires(&arcs, right.samples / 2);
        let p = half[1].points[5];
        let phi = (p[1] - 5.0).atan2(-p[2]);
        assert!((phi.tan() - 0.5).abs() < 0.02, "{phi}");
        assert!((((p[1] - 5.0).hypot(p[2])) - 10.0).abs() < 1e-9);
        // Thicker by half a step (the bands' margin 0 here).
        assert!((half[0].polyline_error - right.half_step).abs() < 1e-12);
        // Both spans swing the same way: the pair stays 5 m apart.
        for k in [0, right.samples / 3, right.samples] {
            let w = right.wires(&arcs, k);
            let d = crate::geometry::distance(w[0].points[5], w[1].points[5]);
            assert!((d - 5.0).abs() < 1e-9, "{k} {d}");
        }
        // Coarse: fewer samples, half a coarse step at most; one arc
        // alone, found by its wire.
        let coarse = swings(&arcs, &cases, WIRE_ARC_COARSE);
        assert!(coarse[1].samples < right.samples);
        assert!(coarse[1].half_step <= WIRE_ARC_COARSE / 2.0 + 1e-12);
        let one = right.only(&arcs, &[1], WIRE_ARC_STEP);
        assert_eq!(one.wires(&arcs, 0).len(), 1);
        assert_eq!(one.arc_of(&arcs, &one.wires(&arcs, 0)[0]), Some(1));
    }
}
