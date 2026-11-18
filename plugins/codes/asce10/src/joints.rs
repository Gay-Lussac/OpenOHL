// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Gusset plates and the welds between them by ASCE 10-97: OpenOHL's core
//! plate checks (`ohl_structures_fe::plates`, before 2026-11-18) for the
//! ASCE 10-97 method, moved unchanged. The geometry (the bolt group in
//! the plate, the distances to the plate's edge, the seam) and the forces
//! come from OpenOHL; the theory page (`docs/theory/joint-plates.md`)
//! explains every step and marks every assumption conservative or not.
//!
//! - **A member's bolts in a plate** ([`bolted`]): bearing 1.5 F_u d t
//!   (4.4; F_u the lower of the plate's and the bolt's), block shear by
//!   Eq. 3.10-1 (P = 0.60 A_v F_u plus A_t F_y; the standard states it for
//!   angles bolted by one leg — the same formula applied to the plate's
//!   block round the bolt group, said), the net section F_y A_n in tension
//!   (3.10.1; holes taken as punched, 1.6 mm larger), the end and edge
//!   distances (4.5.1, 4.5.3 with the bolt's force).
//! - **A weld between two plates** ([`weld`]): ASCE 10-97 gives no weld
//!   resistance (5.2.5 leaves welding procedures to AWS D1.1): not
//!   checked, said.
//! - A plate in compression: the standard gives no rule for a gusset
//!   plate's buckling — "plate buckling not checked", said.

/// A punched hole is taken this much larger in the net section and block
/// shear (ASCE 10-97 3.10.1, 3.10.2: 1/16 in.), m.
const PUNCHED: f64 = 0.0016;

/// A standard hole is this much larger than its bolt (ASCE 10-97 4.1), m.
const STANDARD_HOLE: f64 = 0.0016;

/// What is said of a plate in compression.
pub const BUCKLING_NOT_CHECKED: &str = "plate buckling not checked (neither ASCE 10-97 nor SIA \
                                        263 gives a rule for a gusset plate in compression)";

/// What is said of a weld.
pub const WELD_NOT_CHECKED: &str = "ASCE 10-97 gives no weld resistance (5.2.5 leaves welding \
                                    procedures to AWS D1.1): weld not checked";

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
    /// What is not on the safe side, in words (the part's `not-safe`).
    pub not_safe: Vec<String>,
}

/// What acts on a part beside its force in one load case (sizes, N and
/// N·m; the interface's `part-moments`, 2026-11-18).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Extra {
    /// A bolt group: the member's end shear in the plate's plane.
    pub shear: f64,
    /// A bolt group: the member's moment about the plate's normal at the
    /// bolts' centroid. A weld: the moment of its force about the weld's
    /// middle, bending it along its length.
    pub in_plane: f64,
    /// A bolt group: the moment bending the plate out of its plane. A
    /// weld: the moment about the seam's own line.
    pub out_of_plane: f64,
    /// A bolt group: the force square to the plate.
    pub normal: f64,
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

/// The checks of a plate where a member bolts onto it: its bolt group `g`
/// in plate `s`, the member's axial force `n` at the joint (N, tension
/// positive).
pub fn bolted(g: &BoltGroup, s: &PlateSteel, n: f64) -> Checked {
    let mut out = Checked::default();
    if g.count == 0 {
        out.notes.push("no bolts: not checked".into());
        return out;
    }
    if n < 0.0 {
        out.notes.push(BUCKLING_NOT_CHECKED.into());
    }
    asce10_bolted(g, s, n, &mut out);
    out
}

/// The end distance in the force's direction.
fn end_of(g: &BoltGroup, n: f64) -> Option<f64> {
    g.end[if n >= 0.0 { 0 } else { 1 }]
}

fn mm(x: f64) -> f64 {
    x * 1000.0
}

fn asce10_bolted(g: &BoltGroup, s: &PlateSteel, n: f64, out: &mut Checked) {
    let t = s.thickness;
    let force = n.abs();
    let d = g.diameter;
    let hole = g.hole.unwrap_or_else(|| {
        out.notes
            .push("standard hole, the bolt + 1.6 mm (ASCE 10-97 4.1)".into());
        d + STANDARD_HOLE
    });
    let dh = hole + PUNCHED;
    let nb = f64::from(g.count);
    // Bearing (4.4): 1.5 F_u of the connected part or of the bolt.
    let fu_b = match (s.fu, g.bolt_ultimate) {
        (Some(p), Some(b)) => Some(p.min(b)),
        (Some(p), None) => {
            out.notes
                .push("bearing on the plate's F_u (the bolt's not given)".into());
            Some(p)
        }
        (None, _) => None,
    };
    match fu_b {
        Some(fu) => out.checks.push(PlateCheck {
            what: "bearing on the plate (ASCE 10-97 4.4)".into(),
            demand: force,
            resistance: nb * 1.5 * fu * d * t,
            distance: false,
        }),
        None => out
            .notes
            .push("bearing not checked (no ultimate strength)".into()),
    }
    let e = end_of(g, n);
    // Block shear (Eq. 3.10-1, applied to the plate's block).
    match (s.fu, s.fy, e) {
        (Some(fu), Some(fy), Some(e)) => {
            let k = f64::from(g.per_line.max(1));
            match (g.per_line > 1, g.pitch) {
                (true, None) => out.notes.push("block shear not checked (no pitch)".into()),
                (_, p) => {
                    let p = p.unwrap_or(0.0);
                    let av = 2.0 * t * (e + (k - 1.0) * p - (k - 0.5) * dh).max(0.0);
                    let at = t * (g.span - f64::from(g.lines.max(1) - 1) * dh).max(0.0);
                    out.checks.push(PlateCheck {
                        what: "block shear of the plate (ASCE 10-97 Eq. 3.10-1, stated for \
                               angles: applied to the plate's block round the bolts)"
                            .into(),
                        demand: force,
                        resistance: 0.60 * av * fu + at * fy,
                        distance: false,
                    });
                    // Torn out towards the plate's side edge (2026-11-08):
                    // one shear plane along the line farthest from it, the
                    // tension plane from that line to the side edge across
                    // the others — the angle's own block of Eq. 3.10-1.
                    match g.edge {
                        Some(edge) => {
                            let lines = f64::from(g.lines.max(1));
                            let av1 = t * (e + (k - 1.0) * p - (k - 0.5) * dh).max(0.0);
                            let at1 = t * (edge + g.span - (lines - 0.5) * dh).max(0.0);
                            out.checks.push(PlateCheck {
                                what: "block shear of the plate towards its side edge (ASCE \
                                       10-97 Eq. 3.10-1)"
                                    .into(),
                                demand: force,
                                resistance: 0.60 * av1 * fu + at1 * fy,
                                distance: false,
                            });
                        }
                        None => out.notes.push(
                            "block shear towards the side edge not checked (the plate's side \
                             edge not reached)"
                                .into(),
                        ),
                    }
                }
            }
        }
        (_, _, None) => out
            .notes
            .push("block shear not checked (the plate's edge not reached)".into()),
        _ => out
            .notes
            .push("block shear not checked (no yield or ultimate strength)".into()),
    }
    // Net section (3.10.1), tension.
    if n > 0.0 {
        match s.fy {
            Some(fy) => out.checks.push(PlateCheck {
                what: format!(
                    "net section of the plate (ASCE 10-97 3.10.1; width {})",
                    g.width_words
                ),
                demand: force,
                resistance: fy * t * (g.width - f64::from(g.lines.max(1)) * dh).max(0.0),
                distance: false,
            }),
            None => out
                .notes
                .push("net section not checked (no yield strength)".into()),
        }
    }
    // End and edge distances (4.5.1, 4.5.3; the plate's edge sheared or
    // flame-cut: the larger edge distance).
    if let Some(fu) = s.fu {
        let [end_min, _, edge_min] =
            crate::asce10::bolt_minimums(d, t, fu, Some(force / nb), false, true);
        match e {
            Some(e) => out.checks.push(PlateCheck {
                what: format!(
                    "end distance in the plate {:.0} mm, ≥ {:.0} mm (ASCE 10-97 4.5.1)",
                    mm(e),
                    mm(end_min)
                ),
                demand: end_min,
                resistance: e,
                distance: true,
            }),
            None => out
                .notes
                .push("end distance not checked (the plate's edge not reached)".into()),
        }
        if let Some(f) = g.edge {
            out.checks.push(PlateCheck {
                what: format!(
                    "edge distance in the plate {:.0} mm, ≥ {:.0} mm (ASCE 10-97 4.5.3, Eq. \
                     4.5-7: sheared edge)",
                    mm(f),
                    mm(edge_min)
                ),
                demand: edge_min,
                resistance: f,
                distance: true,
            });
        }
    }
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
}
/// The checks of a weld: none — ASCE 10-97 gives no weld resistance
/// (said).
pub fn weld(w: &WeldInput, force: f64) -> Checked {
    weld_with(w, force, &Extra::default())
}

/// The checks of a weld with the moments of its force about it: none —
/// ASCE 10-97 gives no weld resistance, with or without them (said;
/// 2026-11-18: no rule in ASCE 10-97 for a weld's moment either).
pub fn weld_with(_w: &WeldInput, _force: f64, _x: &Extra) -> Checked {
    Checked {
        checks: Vec::new(),
        notes: vec![WELD_NOT_CHECKED.into()],
        not_safe: Vec::new(),
    }
}

/// Fillet or full penetration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeldKind {
    /// A fillet weld.
    Fillet,
    /// A full penetration weld.
    FullPenetration,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group() -> BoltGroup {
        BoltGroup {
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
        }
    }

    const STEEL: PlateSteel = PlateSteel {
        thickness: 0.010,
        fy: Some(355e6),
        fu: Some(510e6),
    };

    fn find<'a>(c: &'a Checked, what: &str) -> &'a PlateCheck {
        c.checks
            .iter()
            .find(|x| x.what.contains(what))
            .unwrap_or_else(|| panic!("no {what} in {c:?}"))
    }

    /// ASCE 10-97 by hand (invented data): four M16 (hole 18 mm, punched
    /// 19.6 mm) in two lines 60 mm apart, two a line 50 mm apart, 40 mm
    /// to the plate's edge, 10 mm S355 (f_u 510 MPa), bolts f_u 800 MPa:
    /// bearing 4 × 1.5 × 510 × 16 × 10 = 489.6 kN; block shear A_v = 2 × 10
    /// × (40 + 50 − 1.5 × 19.6) = 1212 mm², A_t = 10 × (60 − 19.6) = 404
    /// mm²: 0.6 × 1212 × 510 + 404 × 355 = 514.3 kN; net section 355 × 10 ×
    /// (120 − 2 × 19.6) = 286.84 kN.
    #[test]
    fn asce10_plate_by_hand() {
        let c = bolted(&group(), &STEEL, 200e3);
        let b = find(&c, "bearing");
        assert!((b.resistance - 489.6e3).abs() < 1.0, "{b:?}");
        let s = find(&c, "block shear");
        let want = 0.6 * 1212e-6 * 510e6 + 404e-6 * 355e6;
        assert!((s.resistance - want).abs() < 1.0, "{s:?} {want}");
        // Towards the side edge (30 mm): A_v = 10 × (40 + 50 − 1.5 × 19.6)
        // = 606 mm² (one plane), A_t = 10 × (30 + 60 − 1.5 × 19.6) = 606
        // mm²: 0.6 × 606 × 510 + 606 × 355 = 400.6 kN.
        let side = find(&c, "towards its side edge");
        let want = 0.6 * 606e-6 * 510e6 + 606e-6 * 355e6;
        assert!((side.resistance - want).abs() < 1.0, "{side:?} {want}");
        let n = find(&c, "net section");
        assert!((n.resistance - 286.84e3).abs() < 1.0, "{n:?}");
        // The net section governs at 200 kN: 0.697.
        let (u, g) = c.governing(1.0).unwrap();
        assert!(g.what.contains("net section") && (u - 200.0 / 286.84).abs() < 1e-6);
        // In compression: no net section, buckling said; the end distance
        // the far one (200 mm).
        let c = bolted(&group(), &STEEL, -200e3);
        assert!(!c.checks.iter().any(|x| x.what.contains("net section")));
        assert!(
            c.notes
                .iter()
                .any(|n| n.contains("plate buckling not checked"))
        );
        assert!(find(&c, "end distance in the plate 200 mm").usage(1.0) < 1.0);
        // The end distance by Eq. 4.5-1 at 200 kN: P = 50 kN a bolt, 1.2 ×
        // 50e3 / (510e6 × 0.010) = 11.8 mm < 1.3 d = 20.8 mm.
        let c = bolted(&group(), &STEEL, 200e3);
        let e = find(&c, "end distance");
        assert!((e.demand - 0.0208).abs() < 1e-9 && (e.usage(1.0) - 0.52).abs() < 1e-9);
    }
}
