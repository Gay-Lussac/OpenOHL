// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Gusset plates and the welds between them by SIA 263:2013: OpenOHL's
//! core plate checks (`ohl_structures_fe::plates`, before 2026-11-18) for
//! the SIA 263 method, moved unchanged. The geometry (the bolt group in
//! the plate, the distances to the plate's edge, the seam) and the forces
//! come from OpenOHL; the theory page (`docs/theory/joint-plates.md`)
//! explains every step.
//!
//! - **A member's bolts in a plate** ([`bolted`]): bearing (76) with its
//!   e_1 (p_1 − d_0/2 when the pitch is short, Table 16), the net shear
//!   (82) and block shear (83) of 6.2.4.3, the gross section (38) and in
//!   tension the net section (39) (5.1.2.1), the end and edge distances
//!   of Table 17.
//! - **A weld between two plates** ([`weld`]): 6.3.2 for a fillet weld —
//!   (87) on its throat a, (88) on its contact width s — under the force
//!   it carries spread evenly along it (6.3.1.7); a full penetration weld
//!   is as strong as the weaker plate (6.3.3.4) and not checked (6.3.1.8).
//! - A plate in compression: SIA 263 gives no rule for a gusset plate's
//!   buckling — "plate buckling not checked", said.

use crate::params::{filler_ultimate, gamma_m1, gamma_m2};
use crate::sia263;

/// What is said of a plate in compression.
pub const BUCKLING_NOT_CHECKED: &str = "plate buckling not checked (neither ASCE 10-97 nor SIA \
                                        263 gives a rule for a gusset plate in compression)";

/// A member's bolt group in a plate (SI).
#[derive(Debug, Clone, PartialEq)]
pub struct BoltGroup {
    /// How many bolts.
    pub count: u32,
    /// Their diameter, m.
    pub diameter: f64,
    /// Their hole, m.
    pub hole: Option<f64>,
    /// Gauge lines.
    pub lines: u32,
    /// Bolts along each line.
    pub per_line: u32,
    /// Pitch, m.
    pub pitch: Option<f64>,
    /// Between the outer gauge lines, m.
    pub span: f64,
    /// End distance in tension and in compression, m.
    pub end: [Option<f64>; 2],
    /// Edge distance across the force, m.
    pub edge: Option<f64>,
    /// The plate's width a net section crosses, m.
    pub width: f64,
    /// How that width was taken.
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
    /// What, with the clause.
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

/// The checks of one plate part and what they assumed or left out.
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

/// The end distance in the force's direction.
fn end_of(g: &BoltGroup, n: f64) -> Option<f64> {
    g.end[if n >= 0.0 { 0 } else { 1 }]
}

fn mm(x: f64) -> f64 {
    x * 1000.0
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
    sia263_bolted(g, s, n, &mut out);
    out
}

fn sia263_bolted(g: &BoltGroup, s: &PlateSteel, n: f64, out: &mut Checked) {
    let t = s.thickness;
    let force = n.abs();
    let d = g.diameter;
    let d0 = match g.hole.or_else(|| sia263::standard_hole(d)) {
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
            let one = (0.85 * e / d0).min(2.4) * fu * d * t / gamma_m2();
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
                resistance: 0.9 * fu / (gamma_m2() * 3f64.sqrt()) * av_net,
                distance: false,
            });
            out.checks.push(PlateCheck {
                what: "block shear of the plate (SIA 263 6.2.4.3, (83))".into(),
                demand: force,
                resistance: (0.9 * fu * at + fy / 3f64.sqrt() * av) / gamma_m2(),
                distance: false,
            });
            // Torn out towards the plate's side edge: one shear plane,
            // the tension plane to the side edge.
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
                        resistance: (0.9 * fu * at1 + fy / 3f64.sqrt() * av1) / gamma_m2(),
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
            resistance: fy * a / gamma_m1(),
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
                    / gamma_m2(),
                distance: false,
            }),
            None => out
                .notes
                .push("net section not checked (no ultimate strength)".into()),
        }
    }
    // End and edge distances (Table 17).
    match sia263::bolt_minimums(d) {
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

/// A fillet or a full penetration weld.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeldKind {
    /// A fillet weld.
    Fillet,
    /// A full penetration weld.
    FullPenetration,
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
/// the smaller of the angle and its supplement (the smaller s, on the
/// safe side), m.
pub fn contact_width(a: f64, angle: f64) -> f64 {
    let theta = angle.clamp(0.0, 180.0).min(180.0 - angle.clamp(0.0, 180.0));
    a / (theta.to_radians() / 2.0).cos()
}

/// The checks of a weld carrying `force` (N: the resultant it carries).
pub fn weld(w: &WeldInput, force: f64) -> Checked {
    let mut out = Checked::default();
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
            filler_ultimate() / 1e6
        ),
        demand: force,
        resistance: a * l * 0.6 * filler_ultimate() / gamma_m2(),
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
            resistance: s * l * 0.8 * fy / gamma_m2(),
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
