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
//! - **A weld between two plates** ([`weld`], [`weld_with`]): 6.3.2 for a
//!   fillet weld — (87) on its throat a, (88) on its contact width s —
//!   under the force per unit length at its most loaded end: the force it
//!   carries spread evenly and, since 2026-11-18, the moments of that force
//!   about it, elastic ([`line_force`]; 6.3.1.7, 6.3.2.1); a full penetration weld
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

/// The effective length factor of a gusset plate taken as a strut in
/// compression (2026-11-18; neither ASCE 10-97 nor SIA 263 gives a rule):
/// 2, the column fixed at one end and free to sway at the other — the
/// largest of the classic cases (Timoshenko & Gere, *Theory of Elastic
/// Stability*, 2nd ed., 1961, §2.1); a plate held at both ends buckles
/// over less. An assumption on the safe side.
pub const PLATE_K: f64 = 2.0;

/// Whitmore's width of a bolt group (Whitmore, R. E., 1952, *Experimental
/// investigation of stresses in gusset plates*, University of Tennessee
/// Engineering Experiment Station Bulletin No. 16): lines spread at 30°
/// from the outer bolts of the first row to the last row — the span
/// between the outer gauge lines plus 2 L tan 30°, L the bolts' length
/// along the force ((n − 1) p). None: no pitch known for several bolts a
/// line, or one row (L = 0) of one gauge line (no width).
pub fn whitmore_width(g: &BoltGroup) -> Option<f64> {
    let l = if g.per_line > 1 {
        f64::from(g.per_line - 1) * g.pitch?
    } else {
        0.0
    };
    let w = g.span + 2.0 * l * 30f64.to_radians().tan();
    (w > 0.0).then_some(w)
}

/// The checks of a plate where a member bolts onto it ([`bolted`]) with
/// what else acts on it (`x`: a frame member's end shear and moments at
/// its bolts) and the plate's free length in compression (`free`, m;
/// 2026-11-18): in compression the plate's buckling as a strut
/// ([`plate_buckling`]).
///
/// With a shear or moments (on the safe side, the sizes added): the
/// bolts' bearing under the force of the most loaded bolt times their
/// number, |N| + V + M k ([`moment_factor`]: the bolt group elastic about
/// its centroid), its end distance the smaller of the end and edge
/// distances (the force's direction then not along the member); every
/// other check under |N| + V + 6 M / w + 6 M_out / t (the plate's section
/// of width w, as `g.width`, bent in its plane and out of it), in tension
/// and in compression both (the moment puts one edge in each). The force
/// square to the plate and the moment bending it out of its plane pull on
/// the bolts: bolts in tension not checked, said not on the safe side.
pub fn bolted_with(g: &BoltGroup, s: &PlateSteel, n: f64, x: &Extra, free: Option<f64>) -> Checked {
    let more = x.shear > 0.0 || x.in_plane > 0.0 || x.out_of_plane > 0.0 || x.normal > 0.0;
    if !more || g.count == 0 {
        let mut out = bolted(g, s, n);
        if n < 0.0 && g.count > 0 {
            plate_buckling(g, s, n, free, &mut out);
        }
        return out;
    }
    let t = s.thickness;
    let bend = |m: f64, w: f64| {
        if m > 0.0 {
            if w > 0.0 { 6.0 * m / w } else { f64::INFINITY }
        } else {
            0.0
        }
    };
    let ns = n.abs() + x.shear + bend(x.in_plane, g.width) + bend(x.out_of_plane, t);
    let k = moment_factor(g);
    // Said when it adds 1 % or more to |N| + V (as members' moments are).
    let base = n.abs() + x.shear;
    let matters = |add: f64| add > 0.0 && add >= 0.01 * base;
    // One bolt is a pin: the moment does not pass through it (its bearing
    // under |N| + V); said when the moment matters.
    let nb = base
        + if x.in_plane > 0.0 && k.is_finite() {
            x.in_plane * k
        } else {
            0.0
        };
    let words = format!(
        " with its end shear {:.1} kN and moments {:.2} kN m in the plate's plane, {:.2} kN m \
         out of it",
        x.shear / 1000.0,
        x.in_plane / 1000.0,
        x.out_of_plane / 1000.0
    );
    // Both signs when a moment bends the plate (an edge in each) by 1 %
    // or more of |N| + V.
    let signs: Vec<f64> = if matters(bend(x.in_plane, g.width) + bend(x.out_of_plane, t)) {
        vec![1.0, -1.0]
    } else if n < 0.0 {
        vec![-1.0]
    } else {
        vec![1.0]
    };
    let mut out = Checked::default();
    for sign in signs {
        let mut c = bolted(g, s, sign * ns);
        if sign < 0.0 {
            plate_buckling(g, s, sign * ns, free, &mut c);
        }
        // Bearing: the most loaded bolt's force, the nearer edge.
        let near = |e: Option<f64>| match (e, g.edge) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, None) => a,
            (None, b) => b,
        };
        let gb = BoltGroup {
            end: [near(g.end[0]), near(g.end[1])],
            ..g.clone()
        };
        let cb = bolted(&gb, s, sign * nb);
        c.checks.retain(|p| !p.what.starts_with("bearing"));
        c.checks.extend(
            cb.checks
                .into_iter()
                .filter(|p| p.what.starts_with("bearing")),
        );
        for p in &mut c.checks {
            if !p.distance {
                p.what.push_str(&words);
            }
        }
        for w in c.notes {
            if !out.notes.contains(&w) {
                out.notes.push(w);
            }
        }
        for w in c.not_safe {
            if !out.not_safe.contains(&w) {
                out.not_safe.push(w);
            }
        }
        out.checks.extend(c.checks);
    }
    out.notes.push(
        "a frame member's end shear and moments: sizes added to its axial force, on the safe \
         side (the bearing by the most loaded bolt, elastic, at the nearer of the end and edge \
         distances)"
            .into(),
    );
    if !k.is_finite() && matters(bend(x.in_plane, g.width)) {
        out.not_safe.push(format!(
            "one bolt cannot carry the member's end moment in the plate's plane ({:.2} kN m): \
             a pin there, the frame end of the analysis not what the joint does",
            x.in_plane / 1000.0
        ));
    }
    if matters(x.normal + bend(x.out_of_plane, t)) {
        out.not_safe.push(format!(
            "its force square to the plate ({:.1} kN) and the moment bending the plate out of \
             its plane ({:.2} kN m) pull on its bolts: bolts in tension not checked",
            x.normal / 1000.0,
            x.out_of_plane / 1000.0
        ));
    }
    out
}

/// How much a moment M about the plate's normal adds to the force of the
/// bolt group's most loaded bolt times their number: M k with k = n r_max
/// / Σ r² — the group elastic about its centroid, the bolts on a regular
/// grid of its gauge lines (`span` apart at the outer ones) and rows
/// (`pitch` apart), m⁻¹. A group not filling that grid: Σ r² taken as
/// r_max² alone (on the safe side); one bolt: no lever, infinite.
pub fn moment_factor(g: &BoltGroup) -> f64 {
    let lines = g.lines.max(1);
    let rows = g.per_line.max(1);
    let gauge = if lines > 1 {
        g.span / f64::from(lines - 1)
    } else {
        0.0
    };
    let pitch = g.pitch.unwrap_or(0.0);
    let mut sum = 0.0;
    let mut rmax: f64 = 0.0;
    for i in 0..lines {
        for k in 0..rows {
            let y = (f64::from(i) - f64::from(lines - 1) / 2.0) * gauge;
            let x = (f64::from(k) - f64::from(rows - 1) / 2.0) * pitch;
            sum += x * x + y * y;
            rmax = rmax.max((x * x + y * y).sqrt());
        }
    }
    if g.count != lines * rows {
        sum = rmax * rmax;
    }
    if sum <= 0.0 {
        return f64::INFINITY;
    }
    f64::from(g.count) * rmax / sum
}

/// SIA 263's modulus of elasticity of steel, 210 kN/mm² (3.2.2.1).
const E_STEEL: f64 = 210e9;

/// A plate in compression as a strut (2026-11-18): neither SIA 263 nor
/// ASCE 10-97 gives a rule for a gusset plate's buckling — an assumption
/// on the safe side, said: a strut of Whitmore's width
/// ([`whitmore_width`]; at most the width the plate's sections take,
/// `g.width`), the plate's thickness, over its free length `free` with K
/// = [`PLATE_K`], by SIA 263's buckling (4.5.1: N_K,Rd = χ_K f_y A /
/// γ_M1, λ̄_K = (K L / i) / (π √(E/f_y)), i = t / √12; curve c, the
/// curve of Figure 7 for solid sections, "sections pleines"). Without
/// its free length or width: not checked, said not on the safe side.
fn plate_buckling(g: &BoltGroup, s: &PlateSteel, n: f64, free: Option<f64>, out: &mut Checked) {
    out.notes.retain(|x| x != BUCKLING_NOT_CHECKED);
    let t = s.thickness;
    // One bolt (one row of one line) has no Whitmore width: Whitmore's
    // spread is from a first row to a last one — not checked, said.
    let (Some(l), Some(w), Some(fy)) = (free, whitmore_width(g), s.fy) else {
        out.not_safe.push(format!(
            "plate buckling not checked ({}; neither SIA 263 nor ASCE 10-97 gives a rule)",
            match (free, whitmore_width(g), s.fy) {
                (None, _, _) => "its free length not known",
                (_, None, _) => "no Whitmore width: one bolt, or no pitch",
                _ => "no yield strength",
            }
        ));
        return;
    };
    let b = w.min(g.width);
    let i = t / 12f64.sqrt();
    let lambda = PLATE_K * l / i;
    let lambda_bar = lambda / (std::f64::consts::PI * (E_STEEL / fy).sqrt());
    let chi = sia263::chi(lambda_bar, sia263::Curve::C.alpha());
    out.checks.push(PlateCheck {
        what: format!(
            "plate buckling as a strut (an assumption, on the safe side — no rule in SIA 263 or \
             ASCE 10-97): Whitmore width {:.0} mm{}, t {:.0} mm, free length {:.0} mm, K {:.0}, \
             λ̄_K {:.2}, χ_K {:.3} (SIA 263 4.5.1, curve c)",
            mm(b),
            if b < w { " (the sections' width)" } else { "" },
            mm(t),
            mm(l),
            PLATE_K,
            lambda_bar,
            chi
        ),
        demand: n.abs(),
        resistance: chi * fy * b * t / gamma_m1(),
        distance: false,
    });
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

/// The checks of a weld carrying `force` (N: the resultant it carries)
/// and no moment.
pub fn weld(w: &WeldInput, force: f64) -> Checked {
    weld_with(w, force, &Extra::default())
}

/// The force per unit length a weld of length `l` (m) and throat or
/// contact width `width` (m) takes at its most loaded end, N/m: its force
/// `force` spread evenly (F / L) plus, elastically, the moment of that
/// force about the weld's middle bending it along its length (6 M / L²,
/// the line's section modulus L² / 6) and the moment about the seam's own
/// line taken by the section `width` wide bent across (6 T / (L w)) —
/// the sizes added (on the safe side: their directions may differ). SIA
/// 263 6.3.1.7 admits a simplified distribution of the stresses; 6.3.2.1
/// checks the resultant per unit length (Figure 33).
pub fn line_force(force: f64, x: &Extra, l: f64, width: f64) -> f64 {
    if l <= 0.0 {
        return f64::INFINITY;
    }
    let mut q = force.abs() / l + 6.0 * x.in_plane / (l * l);
    if x.out_of_plane > 0.0 {
        q += if width > 0.0 {
            6.0 * x.out_of_plane / (l * width)
        } else {
            f64::INFINITY
        };
    }
    q
}

/// The checks of a weld carrying `force` (N: the resultant it carries)
/// with the moments of that force about it (`x`, 2026-11-18): (87) and
/// (88) on the force per unit length at its most loaded end
/// ([`line_force`]), as an equivalent force (that force per unit length
/// times the weld's length) against the resistance of the whole length.
pub fn weld_with(w: &WeldInput, force: f64, x: &Extra) -> Checked {
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
    let moments = x.in_plane > 0.0 || x.out_of_plane > 0.0;
    out.notes.push(if moments {
        "the force per unit length at the weld's most loaded end: its force spread evenly, \
         its moments about the weld's middle and about the seam taken elastically (SIA 263 \
         6.3.1.7, 6.3.2.1)"
            .into()
    } else {
        "the force spread evenly along the weld (SIA 263 6.3.1.7)".into()
    });
    if x.out_of_plane > 0.0 {
        out.not_safe.push(format!(
            "the weld bent about its seam ({:.2} kN m): taken by its throat bent across, \
             elastic — a fillet weld on one side only, bent so, should be avoided (SIA 263 \
             6.3.6.5)",
            x.out_of_plane / 1000.0
        ));
    }
    // The equivalent force: the force per unit length at the most loaded
    // end over the whole length (without moments: the force itself).
    let words = |w: f64| {
        if moments {
            format!(
                "; {:.1} kN/m at its most loaded end: F/L {:.1}, 6M/L² {:.1}, 6T/(L·{:.1} mm) \
                 {:.1}",
                line_force(force, x, l, w) / 1000.0,
                force.abs() / l / 1000.0,
                6.0 * x.in_plane / (l * l) / 1000.0,
                mm(w),
                6.0 * x.out_of_plane / (l * w) / 1000.0
            )
        } else {
            String::new()
        }
    };
    let demand = |w: f64| {
        if moments {
            line_force(force, x, l, w) * l
        } else {
            force
        }
    };
    out.checks.push(PlateCheck {
        what: format!(
            "fillet weld a {:.0} mm × {:.0} mm, throat (SIA 263 6.3.2.2, (87), f_uE {:.0} \
             N/mm²{})",
            mm(a),
            mm(l),
            filler_ultimate() / 1e6,
            words(a)
        ),
        demand: demand(a),
        resistance: a * l * 0.6 * filler_ultimate() / gamma_m2(),
        distance: false,
    });
    let s = contact_width(a, w.angle);
    match w.fy {
        Some(fy) => out.checks.push(PlateCheck {
            what: format!(
                "fillet weld s {:.1} mm, contact (SIA 263 6.3.2.2, (88){})",
                mm(s),
                words(s)
            ),
            demand: demand(s),
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
