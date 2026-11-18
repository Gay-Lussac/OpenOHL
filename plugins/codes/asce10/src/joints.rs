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
    // Both signs when a moment bends the plate (an edge in each).
    let signs: Vec<f64> = if x.in_plane > 0.0 || x.out_of_plane > 0.0 {
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

/// ASCE 10-97's modulus of elasticity of steel, 200 000 MPa (Appendix A).
const E_STEEL: f64 = 200e9;

/// A plate in compression as a strut (2026-11-18): neither ASCE 10-97 nor
/// SIA 263 gives a rule for a gusset plate's buckling — an assumption on
/// the safe side, said: a strut of Whitmore's width ([`whitmore_width`];
/// at most the width the plate's sections take, `g.width`), the plate's
/// thickness, over its free length `free` with K = [`PLATE_K`], by ASCE
/// 10-97's column curve (3.6: F_a = [1 − ½ (KL/r / C_c)²] F_y up to C_c =
/// π √(2E/F_y), π² E / (KL/r)² beyond; r = t / √12). Without its free
/// length or width: not checked, said not on the safe side.
fn plate_buckling(g: &BoltGroup, s: &PlateSteel, n: f64, free: Option<f64>, out: &mut Checked) {
    out.notes.retain(|x| x != BUCKLING_NOT_CHECKED);
    let t = s.thickness;
    // One bolt (one row of one line) has no Whitmore width: Whitmore's
    // spread is from a first row to a last one — not checked, said.
    let (Some(l), Some(w), Some(fy)) = (free, whitmore_width(g), s.fy) else {
        out.not_safe.push(format!(
            "plate buckling not checked ({}; neither ASCE 10-97 nor SIA 263 gives a rule)",
            match (free, whitmore_width(g), s.fy) {
                (None, _, _) => "its free length not known",
                (_, None, _) => "no Whitmore width: one bolt, or no pitch",
                _ => "no yield strength",
            }
        ));
        return;
    };
    let b = w.min(g.width);
    let r = t / 12f64.sqrt();
    let kl_r = PLATE_K * l / r;
    let fa = crate::asce10::column_stress(fy, E_STEEL, kl_r);
    out.checks.push(PlateCheck {
        what: format!(
            "plate buckling as a strut (an assumption, on the safe side — no rule in ASCE 10-97 \
             or SIA 263): Whitmore width {:.0} mm{}, t {:.0} mm, free length {:.0} mm, K {:.0}, \
             KL/r {:.0}, F_a {:.0} N/mm² (ASCE 10-97 3.6)",
            mm(b),
            if b < w { " (the sections' width)" } else { "" },
            mm(t),
            mm(l),
            PLATE_K,
            kl_r,
            fa / 1e6
        ),
        demand: n.abs(),
        resistance: fa * b * t,
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

    /// A gusset plate in compression as a strut (2026-11-18; an
    /// assumption: no rule in ASCE 10-97), by hand: the same plate, 200 kN
    /// compression, free length 150 mm. Whitmore's width 60 + 2 × 50 ×
    /// tan 30° = 117.7 mm (below the sections' 120 mm), r = 10/√12 =
    /// 2.887 mm, KL/r = 2 × 150 / 2.887 = 103.9; C_c = π √(2 × 200 000 /
    /// 355) = 105.5, F_a = (1 − ½ (103.9/105.5)²) × 355 = 182.6 N/mm²: 182.6
    /// × 117.7 × 10 = 215.0 kN. Without its free length, or one bolt: not
    /// checked, said not on the safe side; in tension nothing more.
    #[test]
    fn a_plate_in_compression_as_a_strut() {
        let free = Some(0.150);
        let c = bolted_with(&group(), &STEEL, -200e3, &Extra::default(), free);
        let b = 0.060 + 2.0 * 0.050 * 30f64.to_radians().tan();
        assert!((whitmore_width(&group()).unwrap() - b).abs() < 1e-12);
        let kl_r = 2.0 * 0.150 / (0.010 / 12f64.sqrt());
        let cc = std::f64::consts::PI * (2.0 * 200e9 / 355e6f64).sqrt();
        let fa = (1.0 - 0.5 * (kl_r / cc).powi(2)) * 355e6;
        let k = find(&c, "plate buckling as a strut");
        assert!((k.resistance - fa * b * 0.010).abs() < 1e-6, "{k:?}");
        assert!((k.resistance - 215.0e3).abs() < 0.2e3, "{k:?}");
        assert!((k.demand - 200e3).abs() < 1e-9);
        assert!(!c.notes.iter().any(|n| n == BUCKLING_NOT_CHECKED));
        assert!(c.not_safe.is_empty());
        let c = bolted_with(&group(), &STEEL, -200e3, &Extra::default(), None);
        assert!(
            c.not_safe
                .iter()
                .any(|n| n.contains("free length not known"))
        );
        let one = BoltGroup {
            count: 1,
            lines: 1,
            per_line: 1,
            pitch: None,
            span: 0.0,
            ..group()
        };
        let c = bolted_with(&one, &STEEL, -20e3, &Extra::default(), free);
        assert!(c.not_safe.iter().any(|n| n.contains("no Whitmore width")));
        let c = bolted_with(&group(), &STEEL, 200e3, &Extra::default(), free);
        assert!(!c.checks.iter().any(|x| x.what.contains("strut")));
    }

    /// A frame member's end shear and moment in the plate (2026-11-18), by
    /// hand: the same plate, 100 kN tension, 10 kN shear in its plane,
    /// 1 kN m about its normal. The four bolts on a grid ±25 mm along and
    /// ±30 mm across: Σ r² = 4 × 1 525 = 6 100 mm², r_max = 39.05 mm, k = 4
    /// × 39.05 / 6 100 = 25.61 /m — bearing under 100 + 10 + 25.61 =
    /// 135.61 kN (of 489.6 kN); the other checks under 100 + 10 + 6 × 1 /
    /// 0.120 = 160 kN, in tension (net section 160 of 286.84 kN) and in
    /// compression too (the moment puts an edge in each: the strut 160 of
    /// 215.0 kN governs). A force square to the plate: bolts in tension
    /// not checked, said.
    #[test]
    fn a_frame_members_shear_and_moment_in_the_plate() {
        let g = group();
        let k = moment_factor(&g);
        let r = (0.025f64.powi(2) + 0.030f64.powi(2)).sqrt();
        assert!((k - 4.0 * r / (4.0 * r * r)).abs() < 1e-9, "{k}");
        let x = Extra {
            shear: 10e3,
            in_plane: 1e3,
            ..Extra::default()
        };
        let c = bolted_with(&g, &STEEL, 100e3, &x, Some(0.150));
        let b = find(&c, "bearing");
        assert!((b.demand - (110e3 + 1e3 * k)).abs() < 1e-6, "{b:?}");
        assert!((b.demand - 135.61e3).abs() < 0.01e3, "{b:?}");
        assert!((b.resistance - 489.6e3).abs() < 1.0);
        let net = find(&c, "net section");
        assert!((net.demand - 160e3).abs() < 1e-6, "{net:?}");
        let strut = find(&c, "as a strut");
        assert!((strut.demand - 160e3).abs() < 1e-6, "{strut:?}");
        let (u, gov) = c.governing(1.0).unwrap();
        assert!(gov.what.contains("as a strut"), "{gov:?}");
        assert!((u - 160.0 / 215.0).abs() < 0.002, "{u}");
        assert!(c.not_safe.is_empty(), "{c:?}");
        // Without: the axial force alone (the check of before).
        let plain = bolted_with(&g, &STEEL, 100e3, &Extra::default(), Some(0.150));
        assert_eq!(plain, bolted(&g, &STEEL, 100e3));
        assert!(u > plain.governing(1.0).unwrap().0);
        let pull = Extra {
            normal: 2e3,
            ..Extra::default()
        };
        let c = bolted_with(&g, &STEEL, 100e3, &pull, Some(0.150));
        assert!(
            c.not_safe
                .iter()
                .any(|n| n.contains("bolts in tension not checked"))
        );
        // Below 1 % of |N| + V: not said.
        let tiny = Extra {
            normal: 0.5e3,
            ..Extra::default()
        };
        let c = bolted_with(&g, &STEEL, 100e3, &tiny, Some(0.150));
        assert!(c.not_safe.is_empty(), "{c:?}");
        // One bolt cannot take a moment.
        let one = BoltGroup {
            count: 1,
            lines: 1,
            per_line: 1,
            pitch: None,
            span: 0.0,
            ..group()
        };
        assert!(moment_factor(&one).is_infinite());
        // One bolt is a pin: its bearing under |N| + V, the moment said.
        let c = bolted_with(&one, &STEEL, 10e3, &x, Some(0.150));
        assert!((find(&c, "bearing").demand - 20e3).abs() < 1e-6, "{c:?}");
        assert!(
            c.not_safe
                .iter()
                .any(|n| n.contains("one bolt cannot carry"))
        );
    }
}
