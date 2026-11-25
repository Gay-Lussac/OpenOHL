// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Gusset plates and the welds between them (owner 2026-11-04: "add weld
//! size and length and the plate checks"): the resistances of a plate
//! where a member bolts onto it and of a weld between two plates, by the
//! model's capacity method. Built, in active development, not approved
//! yet. The geometry (the bolt group in the plate, the distances to the
//! plate's edge, the seam) and the forces (the analysis' member forces at
//! the joint) come from the structure application
//! (`ohl_structure_link::plate_checks`); the theory page
//! (`docs/theory/joint-plates.md`) explains every step and marks every
//! assumption conservative or not.
//!
//! Per member bolting onto a plate ([`bolted`]) and per weld between two
//! plates ([`weld`]), since 2026-11-18 every design code's checks are its
//! plugin's joint check (`joint-check`, `joint-check-full`): **ASCE 10-97**
//! (the default) in the bundled plugin `asce10` (bearing, block shear, the
//! net section, the end and edge distances; no weld resistance in the
//! standard, said), **SIA 263:2013** in the bundled plugin `sia263` — the
//! same checks moved there (`plugins/codes`), run by
//! `ohl_structure_link::plate_checks`. Here: the parts' records, and
//! **Simple**: not checked (said); a design code without its plugin run:
//! not checked, said ([`NOT_RUN`]).
//!
//! A plate in compression: neither standard gives a rule for a gusset
//! plate's buckling — "plate buckling not checked", said.

use ohl_structures::connections::WeldKind;

use crate::model::{CapacityMethod, Model};

/// What plate checks say when the capacity method has none.
pub const SIMPLE_NOT_CHECKED: &str =
    "simple check: plates and welds not checked (not a design code)";

/// What the core says of a joint part of a design code's (a plugin's)
/// when no plugin was run (the runners send them to its joint check).
pub const NOT_RUN: &str = "NOT CHECKED, NOT ON THE SAFE SIDE: the joint check is the design code's plugin's, not run here, so an overloaded joint would not be found; run the check with the design code's plugin";

/// What is said of a plate in compression.
pub const BUCKLING_NOT_CHECKED: &str = "plate buckling not checked (neither ASCE 10-97 nor SIA \
                                        263 gives a rule for a gusset plate in compression)";

/// A member's bolt group in a plate, as the plate checks take it (SI).
#[derive(Debug, Clone, PartialEq)]
pub struct BoltGroup {
    /// How many bolts.
    pub count: u32,
    /// Their diameter, m.
    pub diameter: f64,
    /// Their hole, m (none: the code's standard hole).
    pub hole: Option<f64>,
    /// The bolts' ultimate strength, Pa (none: not given).
    pub bolt_ultimate: Option<f64>,
    /// Gauge lines: the bolts across the force (in one cross-section).
    pub lines: u32,
    /// Bolts along each line (the most on one).
    pub per_line: u32,
    /// Between bolts along a line, m (none: not known; one bolt a line:
    /// none needed).
    pub pitch: Option<f64>,
    /// Between the outer gauge lines, m (0: one line).
    pub span: f64,
    /// From the bolts farthest in the force's direction to the plate's
    /// edge in that direction, m: the member in tension (pulling away from
    /// the joint) and in compression (pushing towards it). None: the edge
    /// not reached (a bolt outside the plate, or no outline).
    pub end: [Option<f64>; 2],
    /// From the outer gauge lines to the plate's edge across the force, m
    /// (the nearer side).
    pub edge: Option<f64>,
    /// The plate's width the net section crosses, m (the bolt group's
    /// width plus the code's minimum edge distance each side, or the
    /// plate's own width there when narrower).
    pub width: f64,
    /// How that width was taken, in words.
    pub width_words: String,
}

/// A plate's thickness and steel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlateSteel {
    /// Thickness, m.
    pub thickness: f64,
    /// Yield strength, Pa.
    pub fy: Option<f64>,
    /// Ultimate strength, Pa.
    pub fu: Option<f64>,
}

/// One check: what (with its clause), the force on it and its
/// resistance, N (a distance check: the distance needed and the one
/// there, m).
#[derive(Debug, Clone, PartialEq)]
pub struct PlateCheck {
    /// What, with the clause: "bearing (ASCE 10-97 4.4)".
    pub what: String,
    /// The force (or the distance needed).
    pub demand: f64,
    /// The resistance (or the distance there), before the strength factor.
    pub resistance: f64,
    /// A distance check: the strength factor does not apply.
    pub distance: bool,
}

impl PlateCheck {
    /// The share used, the case's strength factor applied to a resistance.
    pub fn usage(&self, strength: f64) -> f64 {
        let r = if self.distance {
            self.resistance
        } else {
            self.resistance * strength
        };
        if r > 0.0 {
            self.demand / r
        } else if self.demand > 0.0 {
            f64::INFINITY
        } else {
            0.0
        }
    }
}

/// The checks of one plate part (a member's bolts in it, a weld) and what
/// they assumed or left out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Checked {
    /// The checks.
    pub checks: Vec<PlateCheck>,
    /// What was assumed or not checked, in words.
    pub notes: Vec<String>,
}

impl Checked {
    /// The check using most, and its usage (none: nothing checked). A
    /// distance is kept or not: it governs only when not kept (its share
    /// above 1), never as a share of a resistance.
    pub fn governing(&self, strength: f64) -> Option<(f64, &PlateCheck)> {
        let all: Vec<(f64, &PlateCheck)> =
            self.checks.iter().map(|c| (c.usage(strength), c)).collect();
        let order =
            |a: &(f64, &PlateCheck), b: &(f64, &PlateCheck)| crate::check::usage_order(a.0, b.0);
        let counted = all
            .iter()
            .filter(|(u, c)| !c.distance || u.is_nan() || *u > 1.0)
            .copied()
            .max_by(order);
        counted.or_else(|| all.iter().copied().max_by(order))
    }
}

/// The bolts of member `idx` at its end `end` as the member check takes
/// them (the end's connection over the member's design data).
pub fn end_bolts(model: &Model, idx: usize, end: usize) -> Option<crate::design::Bolts> {
    let m = model.members.get(idx)?;
    let e = m.design.ends.get(end).and_then(Option::as_ref);
    crate::member_ends::end_bolts(&m.design, e, &mut Vec::new()).map(|b| b.0)
}

/// The checks of a plate where a member bolts onto it: its bolt group `g`
/// in plate `s`, the member's axial force `n` at the joint (N, tension
/// positive), by `method`.
pub fn bolted(method: CapacityMethod, g: &BoltGroup, _s: &PlateSteel, n: f64) -> Checked {
    let mut out = Checked::default();
    if g.count == 0 {
        out.notes.push("no bolts: not checked".into());
        return out;
    }
    if n < 0.0 {
        out.notes.push(BUCKLING_NOT_CHECKED.into());
    }
    match method {
        CapacityMethod::Simple => out.notes.push(SIMPLE_NOT_CHECKED.into()),
        CapacityMethod::Code => out.notes.push(NOT_RUN.into()),
    }
    out
}

/// A weld between two plates as its check takes it.
#[derive(Debug, Clone, PartialEq)]
pub struct WeldInput {
    /// Fillet or full penetration.
    pub kind: WeldKind,
    /// The fillet's throat a, m (none: not given — not checked).
    pub size: Option<f64>,
    /// Its length, m (given, else the seam's).
    pub length: f64,
    /// The length given (false: the seam's, one side).
    pub length_given: bool,
    /// The angle between the two plates at the seam, degrees (0 to 180).
    pub angle: f64,
    /// The lower yield strength of the two plates, Pa.
    pub fy: Option<f64>,
    /// The two plates' thicknesses, m.
    pub thickness: [f64; 2],
    /// Its fillets (the file's `sides`, 2026-11-25, roadmap O44 J7b): 1 or
    /// 2; none: not known (the check takes the worse of the two).
    pub sides: Option<u32>,
}
/// The checks of a weld by `method`: none here — the simple check is no
/// design code, a design code's weld checks are its plugin's since
/// 2026-11-18 (`plugins/codes`; ASCE 10-97 gives no weld resistance, said
/// by its plugin) — both said.
pub fn weld(method: CapacityMethod, _w: &WeldInput, _force: f64) -> Checked {
    let mut out = Checked::default();
    match method {
        CapacityMethod::Simple => out.notes.push(SIMPLE_NOT_CHECKED.into()),
        CapacityMethod::Code => out.notes.push(NOT_RUN.into()),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No check in the core (2026-11-18: ASCE 10-97's, the plate test
    /// with it, moved into its plugin `asce10`): the simple check and a
    /// design code without its plugin run say so, without a usage.
    #[test]
    fn the_core_checks_no_plate() {
        let g = BoltGroup {
            count: 4,
            diameter: 0.016,
            hole: Some(0.018),
            bolt_ultimate: Some(800e6),
            lines: 2,
            per_line: 2,
            pitch: Some(0.050),
            span: 0.060,
            end: [Some(0.040), Some(0.200)],
            edge: Some(0.030),
            width: 0.120,
            width_words: "the bolts' 60 mm + 2 × 30 mm".into(),
        };
        let steel = PlateSteel {
            thickness: 0.010,
            fy: Some(355e6),
            fu: Some(510e6),
        };
        let c = bolted(CapacityMethod::Simple, &g, &steel, 200e3);
        assert!(c.checks.is_empty() && c.notes[0].contains("not checked"));
        let c = bolted(CapacityMethod::Code, &g, &steel, -200e3);
        assert!(c.checks.is_empty());
        assert!(c.notes.iter().any(|n| n == NOT_RUN));
        assert!(
            c.notes
                .iter()
                .any(|n| n.contains("plate buckling not checked"))
        );
        assert!(c.governing(1.0).is_none());
    }
}
