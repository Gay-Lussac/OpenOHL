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
//! Per member bolting onto a plate ([`bolted`]):
//!
//! - **ASCE 10-97**: bearing 1.5 F_u d t (4.4; F_u the lower of the
//!   plate's and the bolt's), block shear by Eq. 3.10-1 (P = 0.60 A_v
//!   F_u plus A_t F_y; the standard states it for angles bolted by one
//!   leg — the same formula applied to the plate's block round the bolt
//!   group, said), the net section F_y A_n in tension (3.10.1; holes taken
//!   as punched, 1.6 mm larger), the end and edge distances (4.5.1, 4.5.3
//!   with the bolt's force);
//! - **SIA 263:2013**: bearing (76) with its e_1 (p_1 − d_0/2 when the
//!   pitch is short, Table 16), the net shear (82) and block shear (83) of
//!   6.2.4.3, the gross section (38) and in tension the net section (39)
//!   (5.1.2.1), the end and edge distances of Table 17;
//! - **Simple**: not checked (said).
//!
//! A weld between two plates ([`weld`]): SIA 263 6.3.2 for a fillet weld
//! — (87) on its throat a, (88) on its contact width s — under the force
//! it carries spread evenly along it (6.3.1.7); a full penetration weld is
//! as strong as the weaker plate (6.3.3.4) and not checked (6.3.1.8).
//! ASCE 10-97 gives no weld resistance (5.2.5 leaves welding procedures to
//! AWS D1.1): not checked under it, said.
//!
//! A plate in compression: neither standard gives a rule for a gusset
//! plate's buckling — "plate buckling not checked", said.

use ohl_structures::connections::WeldKind;

use crate::model::{CapacityMethod, Model};
use crate::sia263::{GAMMA_M1, GAMMA_M2};

/// A punched hole is taken this much larger in the net section and block
/// shear (ASCE 10-97 3.10.1, 3.10.2: 1/16 in.), m.
const PUNCHED: f64 = 0.0016;

/// A standard hole is this much larger than its bolt (ASCE 10-97 4.1), m.
const STANDARD_HOLE: f64 = 0.0016;

/// The filler metal's tensile strength f_uE (SIA 263 6.3.2.2: "en général
/// 510 N/mm²"), Pa.
pub const FILLER_ULTIMATE: f64 = 510e6;

/// What plate checks say when the capacity method has none.
pub const SIMPLE_NOT_CHECKED: &str =
    "simple check: plates and welds not checked (not a design code)";

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
    crate::asce10::end_bolts(&m.design, e, &mut Vec::new()).map(|b| b.0)
}

/// The checks of a plate where a member bolts onto it: its bolt group `g`
/// in plate `s`, the member's axial force `n` at the joint (N, tension
/// positive), by `method`.
pub fn bolted(method: CapacityMethod, g: &BoltGroup, s: &PlateSteel, n: f64) -> Checked {
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
        CapacityMethod::Asce10 => asce10_bolted(g, s, n, &mut out),
        CapacityMethod::Sia263 => sia263_bolted(g, s, n, &mut out),
    }
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

fn sia263_bolted(g: &BoltGroup, s: &PlateSteel, n: f64, out: &mut Checked) {
    let t = s.thickness;
    let force = n.abs();
    let d = g.diameter;
    let d0 = match g.hole.or_else(|| crate::sia263::standard_hole(d)) {
        Some(h) => {
            if g.hole.is_none() {
                out.notes.push("standard hole (SIA 263 Table 17)".into());
            }
            h
        }
        None => {
            out.notes.push("hole: the bolt + 3 mm".into());
            d + 0.003
        }
    };
    let nb = f64::from(g.count);
    let k = f64::from(g.per_line.max(1));
    let e = end_of(g, n);
    // Bearing (76), Table 16.
    match (s.fu, e) {
        (Some(fu), Some(e1)) => {
            let mut e = e1;
            if g.per_line > 1 {
                match g.pitch {
                    Some(p) if p < e1 + d0 / 2.0 => e = p - d0 / 2.0,
                    Some(_) => {}
                    None => out
                        .notes
                        .push("pitch not known: bearing by e_1 alone".into()),
                }
            }
            let one = (0.85 * e / d0).min(2.4) * fu * d * t / GAMMA_M2;
            out.checks.push(PlateCheck {
                what: "bearing on the plate (SIA 263 Table 16, (76))".into(),
                demand: force,
                resistance: nb * one.max(0.0),
                distance: false,
            });
            let p2 = if g.lines > 1 {
                Some(g.span / f64::from(g.lines - 1))
            } else {
                None
            };
            if g.edge.is_some_and(|e2| e2 < d0) || p2.is_some_and(|p2| p2 < 2.0 * d0) {
                out.notes.push(format!(
                    "(76) asks e_2 ≥ d_0 and p_2 ≥ 2 d_0 ({:.0} mm): not met in the plate",
                    mm(d0)
                ));
            }
        }
        (None, _) => out
            .notes
            .push("bearing not checked (no ultimate strength)".into()),
        (_, None) => out
            .notes
            .push("bearing not checked (the plate's edge not reached)".into()),
    }
    // 6.2.4.3: net shear (82) and block shear (83), the block round the
    // bolt group torn out towards the edge in the force's direction.
    match (s.fu, s.fy, e, g.per_line > 1 && g.pitch.is_none()) {
        (_, _, _, true) => out.notes.push("block shear not checked (no pitch)".into()),
        (Some(fu), Some(fy), Some(e), false) => {
            let p = g.pitch.unwrap_or(0.0);
            let av_net = 2.0 * t * (e + (k - 1.0) * p - (k - 0.5) * d0).max(0.0);
            let av = 2.0 * t * (e + (k - 1.0) * p);
            let at = t * (g.span - f64::from(g.lines.max(1) - 1) * d0).max(0.0);
            out.checks.push(PlateCheck {
                what: "net shear of the plate (SIA 263 6.2.4.3, (82))".into(),
                demand: force,
                resistance: 0.9 * fu / (GAMMA_M2 * 3f64.sqrt()) * av_net,
                distance: false,
            });
            out.checks.push(PlateCheck {
                what: "block shear of the plate (SIA 263 6.2.4.3, (83))".into(),
                demand: force,
                resistance: (0.9 * fu * at + fy / 3f64.sqrt() * av) / GAMMA_M2,
                distance: false,
            });
            // Torn out towards the plate's side edge (2026-11-08): one
            // shear plane, the tension plane to the side edge.
            match g.edge {
                Some(edge) => {
                    let lines = f64::from(g.lines.max(1));
                    let av1 = t * (e + (k - 1.0) * p);
                    let at1 = t * (edge + g.span - (lines - 0.5) * d0).max(0.0);
                    out.checks.push(PlateCheck {
                        what: "block shear of the plate towards its side edge (SIA 263 6.2.4.3, \
                               (83))"
                            .into(),
                        demand: force,
                        resistance: (0.9 * fu * at1 + fy / 3f64.sqrt() * av1) / GAMMA_M2,
                        distance: false,
                    });
                }
                None => out.notes.push(
                    "block shear towards the side edge not checked (the plate's side edge not \
                     reached)"
                        .into(),
                ),
            }
        }
        (_, _, None, false) => out
            .notes
            .push("block shear not checked (the plate's edge not reached)".into()),
        _ => out
            .notes
            .push("block shear not checked (no yield or ultimate strength)".into()),
    }
    // 5.1.2.1: the gross section (38); in tension the net section (39).
    let a = t * g.width;
    match s.fy {
        Some(fy) => out.checks.push(PlateCheck {
            what: format!(
                "gross section of the plate (SIA 263 5.1.2.1, (38); width {})",
                g.width_words
            ),
            demand: force,
            resistance: fy * a / GAMMA_M1,
            distance: false,
        }),
        None => out
            .notes
            .push("gross section not checked (no yield strength)".into()),
    }
    if n > 0.0 {
        match s.fu {
            Some(fu) => out.checks.push(PlateCheck {
                what: format!(
                    "net section of the plate (SIA 263 5.1.2.1, (39); width {})",
                    g.width_words
                ),
                demand: force,
                resistance: 0.9 * fu * t * (g.width - f64::from(g.lines.max(1)) * d0).max(0.0)
                    / GAMMA_M2,
                distance: false,
            }),
            None => out
                .notes
                .push("net section not checked (no ultimate strength)".into()),
        }
    }
    // End and edge distances (Table 17).
    match crate::sia263::bolt_minimums(d) {
        Some([e1_min, _, e2_min]) => {
            match e {
                Some(e) => out.checks.push(PlateCheck {
                    what: format!(
                        "end distance in the plate {:.0} mm, ≥ {:.0} mm (SIA 263 Table 17, e_1)",
                        mm(e),
                        mm(e1_min)
                    ),
                    demand: e1_min,
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
                        "edge distance in the plate {:.0} mm, ≥ {:.0} mm (SIA 263 Table 17, e_2)",
                        mm(f),
                        mm(e2_min)
                    ),
                    demand: e2_min,
                    resistance: f,
                    distance: true,
                });
            }
        }
        None => out.notes.push(format!(
            "distances not checked: SIA 263 Table 17 has no M{:.0}",
            mm(d)
        )),
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

/// The fillet's contact width s from its throat `a` (SIA 263 6.3.2.6,
/// Figure 34: the triangle inscribed in the weld's section, taken
/// isosceles) between plates at `angle` degrees: s = a / cos(θ/2) with θ
/// the smaller of the angle and its supplement (the fillet may lie on
/// either side; the smaller s, on the safe side), m.
pub fn contact_width(a: f64, angle: f64) -> f64 {
    let theta = angle.clamp(0.0, 180.0).min(180.0 - angle.clamp(0.0, 180.0));
    a / (theta.to_radians() / 2.0).cos()
}

/// The checks of a weld carrying `force` (N: the resultant it carries), by
/// `method`.
pub fn weld(method: CapacityMethod, w: &WeldInput, force: f64) -> Checked {
    let mut out = Checked::default();
    match method {
        CapacityMethod::Simple => {
            out.notes.push(SIMPLE_NOT_CHECKED.into());
            return out;
        }
        CapacityMethod::Asce10 => {
            out.notes.push(
                "ASCE 10-97 gives no weld resistance (5.2.5 leaves welding procedures to AWS \
                 D1.1): weld not checked"
                    .into(),
            );
            return out;
        }
        CapacityMethod::Sia263 => {}
    }
    if w.kind == WeldKind::FullPenetration {
        out.notes.push(
            "full penetration weld: as strong as the weaker plate (SIA 263 6.3.3.4), not \
             checked (6.3.1.8)"
                .into(),
        );
        return out;
    }
    let Some(a) = w.size else {
        out.notes
            .push("no weld size given: weld not checked".into());
        return out;
    };
    if !w.length_given {
        out.notes.push(format!(
            "length {:.0} mm: the seam's, one side (not given)",
            mm(w.length)
        ));
    }
    let l = w.length;
    out.notes.push(
        "the force spread evenly along the weld (SIA 263 6.3.1.7); its moment about the seam \
         not taken (not on the safe side)"
            .into(),
    );
    out.checks.push(PlateCheck {
        what: format!(
            "fillet weld a {:.0} mm × {:.0} mm, throat (SIA 263 6.3.2.2, (87), f_uE {:.0} \
             N/mm²)",
            mm(a),
            mm(l),
            FILLER_ULTIMATE / 1e6
        ),
        demand: force,
        resistance: a * l * 0.6 * FILLER_ULTIMATE / GAMMA_M2,
        distance: false,
    });
    let s = contact_width(a, w.angle);
    match w.fy {
        Some(fy) => out.checks.push(PlateCheck {
            what: format!(
                "fillet weld s {:.1} mm, contact (SIA 263 6.3.2.2, (88))",
                mm(s)
            ),
            demand: force,
            resistance: s * l * 0.8 * fy / GAMMA_M2,
            distance: false,
        }),
        None => out
            .notes
            .push("(88) not checked (no yield strength of the plates)".into()),
    }
    let t_min = w.thickness[0].min(w.thickness[1]);
    let t_max = w.thickness[0].max(w.thickness[1]);
    if l < 0.040 {
        out.notes.push(format!(
            "length {:.0} mm below 40 mm (SIA 263 6.3.6.2)",
            mm(l)
        ));
    }
    if a > 0.7 * t_min + 1e-9 {
        out.notes.push(format!(
            "a {:.0} mm above 0.7 × the thinner plate, {:.1} mm (SIA 263 6.3.6.3)",
            mm(a),
            mm(0.7 * t_min)
        ));
    }
    if t_min < 0.004 {
        out.notes
            .push("a plate thinner than 4 mm: SIA 263 6.3 does not apply (6.3.1.3)".into());
    }
    // 6.3.6.4: the recommended least a for one pass, by the thicker plate.
    let a_min = if t_max >= 0.026 {
        Some(0.006)
    } else if t_max >= 0.018 {
        Some(0.005)
    } else if t_max >= 0.013 {
        Some(0.004)
    } else {
        None
    };
    if let Some(m) = a_min
        && a < m - 1e-9
    {
        out.notes.push(format!(
            "a {:.0} mm below the {:.0} mm recommended for a {:.0} mm plate (SIA 263 6.3.6.4)",
            mm(a),
            mm(m),
            mm(t_max)
        ));
    }
    out
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
        let c = bolted(CapacityMethod::Asce10, &group(), &STEEL, 200e3);
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
        let c = bolted(CapacityMethod::Asce10, &group(), &STEEL, -200e3);
        assert!(!c.checks.iter().any(|x| x.what.contains("net section")));
        assert!(
            c.notes
                .iter()
                .any(|n| n.contains("plate buckling not checked"))
        );
        assert!(find(&c, "end distance in the plate 200 mm").usage(1.0) < 1.0);
        // The end distance by Eq. 4.5-1 at 200 kN: P = 50 kN a bolt, 1.2 ×
        // 50e3 / (510e6 × 0.010) = 11.8 mm < 1.3 d = 20.8 mm.
        let c = bolted(CapacityMethod::Asce10, &group(), &STEEL, 200e3);
        let e = find(&c, "end distance");
        assert!((e.demand - 0.0208).abs() < 1e-9 && (e.usage(1.0) - 0.52).abs() < 1e-9);
        // Simple: nothing checked, said.
        let c = bolted(CapacityMethod::Simple, &group(), &STEEL, 200e3);
        assert!(c.checks.is_empty() && c.notes[0].contains("not checked"));
    }

    /// SIA 263 by hand (the same plate): bearing (76) e_1 40 mm, p_1 50 mm
    /// not below e_1 + d_0/2 = 49 mm (e_1 kept): 0.85 × 40/18 = 1.889 < 2.4
    /// → 1.889 × 510 × 16 × 10 / 1.25 = 123.3 kN a bolt, 493.2 kN; (82) A_v,net = 2 × 10 × (40 + 50 − 1.5 × 18) = 1260 mm²: 0.9 ×
    /// 510/(1.25 √3) × 1260 = 267.1 kN; (83) A_t = 10 × (60 − 18) = 420,
    /// A_v = 2 × 10 × 90 = 1800: (0.9 × 510 × 420 + 355/√3 × 1800)/1.25 =
    /// 449.3 kN; (38) 355 × 1200/1.05 = 405.7 kN; (39) 0.9 × 510 × 10 ×
    /// (120 − 36)/1.25 = 308.4 kN.
    #[test]
    fn sia263_plate_by_hand() {
        let c = bolted(CapacityMethod::Sia263, &group(), &STEEL, 200e3);
        let one = 0.85 * 40.0 / 18.0 * 510e6 * 0.016 * 0.010 / 1.25;
        assert!((find(&c, "(76)").resistance - 4.0 * one).abs() < 1.0);
        let r82 = 0.9 * 510e6 / (1.25 * 3f64.sqrt()) * 1260e-6;
        assert!((find(&c, "(82)").resistance - r82).abs() < 1.0);
        let r83 = (0.9 * 510e6 * 420e-6 + 355e6 / 3f64.sqrt() * 1800e-6) / 1.25;
        assert!((find(&c, "(83)").resistance - r83).abs() < 1.0);
        // Towards the side edge: A_t = 10 × (30 + 60 − 1.5 × 18) = 630 mm²,
        // A_v = 10 × 90 = 900 mm² (one plane).
        let side = (0.9 * 510e6 * 630e-6 + 355e6 / 3f64.sqrt() * 900e-6) / 1.25;
        assert!((find(&c, "towards its side edge").resistance - side).abs() < 1.0);
        assert!((find(&c, "(38)").resistance - 355e6 * 1200e-6 / 1.05).abs() < 1.0);
        assert!((find(&c, "(39)").resistance - 0.9 * 510e6 * 840e-6 / 1.25).abs() < 1.0);
        // A short pitch: e = p − d_0/2 = 30 − 9 = 21 mm.
        let short = BoltGroup {
            pitch: Some(0.030),
            ..group()
        };
        let c = bolted(CapacityMethod::Sia263, &short, &STEEL, 200e3);
        let one = 0.85 * 21.0 / 18.0 * 510e6 * 0.016 * 0.010 / 1.25;
        assert!((find(&c, "(76)").resistance - 4.0 * one).abs() < 1.0);
        // Table 17 for M16: e_1 ≥ 25 mm, e_2 ≥ 20 mm.
        let c = bolted(CapacityMethod::Sia263, &group(), &STEEL, 200e3);
        assert!((find(&c, "e_1").demand - 0.025).abs() < 1e-12);
        assert!((find(&c, "e_2").demand - 0.020).abs() < 1e-12);
    }

    /// A fillet weld by SIA 263 6.3.2.2 (invented data): a = 6 mm, 220 mm
    /// long, plates at 90° (s = 6√2 = 8.49 mm), S355: (87) 6 × 220 × 0.6 ×
    /// 510/1.25 = 323.1 kN; (88) 8.485 × 220 × 0.8 × 355/1.25 = 424.1 kN.
    /// ASCE 10-97: no weld rule, not checked (said).
    #[test]
    fn a_fillet_weld_by_sia263() {
        let w = WeldInput {
            kind: WeldKind::Fillet,
            size: Some(0.006),
            length: 0.220,
            length_given: true,
            angle: 90.0,
            fy: Some(355e6),
            thickness: [0.010, 0.010],
        };
        let c = weld(CapacityMethod::Sia263, &w, 100e3);
        let r87 = 0.006 * 0.220 * 0.6 * 510e6 / 1.25;
        assert!((find(&c, "(87)").resistance - r87).abs() < 1.0);
        assert!((r87 - 323.136e3).abs() < 1.0);
        let s = 0.006 * 2f64.sqrt();
        let r88 = s * 0.220 * 0.8 * 355e6 / 1.25;
        assert!((find(&c, "(88)").resistance - r88).abs() < 1e-6);
        let (u, g) = c.governing(1.0).unwrap();
        assert!(g.what.contains("(87)") && (u - 100e3 / r87).abs() < 1e-12);
        // The contact width: 90° → a√2; 120° (its supplement 60°) → a/cos 30°.
        assert!((contact_width(0.006, 120.0) - 0.006 / 30f64.to_radians().cos()).abs() < 1e-12);
        // No size: not checked; full penetration: not checked.
        let none = WeldInput {
            size: None,
            ..w.clone()
        };
        assert!(weld(CapacityMethod::Sia263, &none, 1e3).checks.is_empty());
        let full = WeldInput {
            kind: WeldKind::FullPenetration,
            ..w.clone()
        };
        assert!(weld(CapacityMethod::Sia263, &full, 1e3).checks.is_empty());
        let asce = weld(CapacityMethod::Asce10, &w, 1e3);
        assert!(asce.checks.is_empty() && asce.notes[0].contains("AWS D1.1"));
        // a above 0.7 t: said (6.3.6.3).
        let thin = WeldInput {
            thickness: [0.006, 0.010],
            ..w
        };
        assert!(
            weld(CapacityMethod::Sia263, &thin, 1e3)
                .notes
                .iter()
                .any(|n| n.contains("6.3.6.3"))
        );
    }
}
