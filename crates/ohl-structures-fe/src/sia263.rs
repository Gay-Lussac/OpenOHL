// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Member capacities by **SIA 263:2013** (*Construction en acier / Stahlbau*,
//! Swiss Society of Engineers and Architects) — a capacity method of the
//! structure check beside ASCE 10-97 (`[analysis] capacity =
//! "sia263-2013"`; owner, 2026-11-03). Built, in active development, not
//! approved yet. Clause, table and formula numbers are the standard's;
//! the theory page (`docs/theory/sia263.md`) explains every step.
//!
//! The capacities are **design values** with the resistance factors
//! γ_M1 = 1.05 and γ_M2 = 1.25 (4.1.3) — the load case's strength factor
//! still multiplies them (1 for SIA load cases; owner 2026-11-03).
//!
//! - **Section class** (Tables 5a, 5b) from the compressed elements'
//!   b/t, the class of uniform compression for every check (on the safe
//!   side for bending); class 4: effective widths b_eff = ρ b_c, ρ =
//!   0.9/λ̄_P ≤ 1 (4.5.3.2, 4.5.3.3; k from Table 9: 0.426 an outstand, 4.0
//!   an internal element).
//! - **Flexural buckling** (4.5.1): χ_K by the curves a–d (formula 8,
//!   Table 8, Figure 7), N_K,Rd = χ_K f_y A/γ_M1 (7); class 4 with A_eff
//!   (λ̄_K by 5.3.8). Buckling lengths L_K = L between the joints held
//!   (5.4.1.3; the member's length ratios, else 1, said), single angles
//!   connected eccentrically and crossed angles about the minor axis with
//!   L_K = 0.9 L (5.4.1.6). Torsional and torsional-flexural buckling
//!   (4.5.1.2: "dans des cas spéciaux") by elastic theory, as for ASCE
//!   10-97 ([`crate::asce10::torsional_slenderness`]).
//! - **Single angles bolted by one leg** (owner 2026-11-03: the literal
//!   reading): 5.4.1.6 and the bending from the connection's eccentricity
//!   (5.4.1.2), N·e about both principal axes, in formula (50) (classes
//!   1–3; lateral-torsional buckling about the major axis by Annex B) or
//!   (62)/(63) (class 4, with the imperfections of Figure 3 and the
//!   effective section's centroid shift).
//! - **Frames** (bending): formula (50) in compression, the linear sum
//!   in tension (5.1.6, 5.2: elastic interaction, on the safe side).
//! - **Tension** (5.1.2): the least of f_y A/γ_M1 (38), 0.9 f_u A_net/γ_M2
//!   (39), an angle bolted by one leg 6.2.4.4 (83) or (84), the bolts'
//!   shear and bearing (Table 16).
//! - **Bolts** (6.2.2, Table 16): shear 0.6 or 0.5 f_ub A_s/γ_M2 per plane
//!   by the grade (73, 74; threads in the plane), bearing (76); minimum
//!   distances (Table 17) for the bolt tools ([`bolt_minimums`]).
//!
//! **Missing data are assumed on the safe side and said per member**, as
//! for ASCE 10-97.

use crate::asce10::Capacity;
use crate::design::{Bolts, ConnectedLeg, EndDesign, MemberClass, MemberDesign};
use crate::member_ends;
use crate::model::{Member, Model, Section};

/// The method's name in files and results.
pub const NAME: &str = "sia263-2013";

/// The standard's name in results.
pub const TITLE: &str = "SIA 263:2013";

/// Resistance factor of members (4.1.3).
pub const GAMMA_M1: f64 = 1.05;

/// Resistance factor of net sections and connections (4.1.3).
pub const GAMMA_M2: f64 = 1.25;

/// The reference yield strength of ε = √(235/f_y), Pa (Table 5b).
const FY_REF: f64 = 235e6;

/// Buckling curves (Table 8, Figure 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    /// Curve a.
    A,
    /// Curve b.
    B,
    /// Curve c.
    C,
    /// Curve d.
    D,
}

impl Curve {
    /// The imperfection factor α_K (Table 8).
    pub fn alpha(self) -> f64 {
        match self {
            Curve::A => 0.21,
            Curve::B => 0.34,
            Curve::C => 0.49,
            Curve::D => 0.76,
        }
    }

    /// The equivalent bow w_0 over the bar's length, elastic method EE
    /// (Figure 3): 1/300, 1/250, 1/200, 1/150.
    pub fn bow(self) -> f64 {
        1.0 / match self {
            Curve::A => 300.0,
            Curve::B => 250.0,
            Curve::C => 200.0,
            Curve::D => 150.0,
        }
    }

    /// Its letter.
    pub fn letter(self) -> char {
        match self {
            Curve::A => 'a',
            Curve::B => 'b',
            Curve::C => 'c',
            Curve::D => 'd',
        }
    }
}

/// The reduction factor χ_K for the relative slenderness λ̄_K and the
/// imperfection factor α (formula 8): Φ = 0.5 [1 + α (λ̄ − 0.2) + λ̄²],
/// χ = 1/(Φ + √(Φ² − λ̄²)) ≤ 1.
pub fn chi(lambda_bar: f64, alpha: f64) -> f64 {
    let phi = 0.5 * (1.0 + alpha * (lambda_bar - 0.2) + lambda_bar * lambda_bar);
    (1.0 / (phi + (phi * phi - lambda_bar * lambda_bar).max(0.0).sqrt())).min(1.0)
}

/// The lateral-torsional reduction factor χ_D (formula 10): Φ_D = 0.5 [1 +
/// α_D (λ̄_D − 0.4) + λ̄_D²], χ_D = 1/(Φ_D + √(Φ_D² − λ̄_D²)) ≤ 1.
pub fn chi_lt(lambda_bar: f64, alpha: f64) -> f64 {
    let phi = 0.5 * (1.0 + alpha * (lambda_bar - 0.4) + lambda_bar * lambda_bar);
    (1.0 / (phi + (phi * phi - lambda_bar * lambda_bar).max(0.0).sqrt())).min(1.0)
}

/// ε = √(235 MPa/f_y) (Table 5b).
pub fn epsilon(fy: f64) -> f64 {
    (FY_REF / fy).sqrt()
}

/// The effective width factor ρ = 0.9/λ̄_P ≤ 1 of a plate of b/t with the
/// buckling coefficient k (4.5.3.3): λ̄_P = (b/t)/(28.4 ε √k).
pub fn rho(b_t: f64, k: f64, fy: f64) -> f64 {
    let lp = b_t / (28.4 * epsilon(fy) * k.sqrt());
    if lp <= 0.0 { 1.0 } else { (0.9 / lp).min(1.0) }
}

/// k of an outstand and of an internal element in uniform compression
/// (Table 9, ψ = 1).
const K_OUTSTAND: f64 = 0.426;
const K_INTERNAL: f64 = 4.0;

/// η of a moment distribution (formula 94, ψ = M_min/M_max with signs):
/// 1.75 − 1.05 ψ + 0.3 ψ², valid for ψ ≥ −0.5; below, its value at −0.5
/// (Figure 41 ends there; the lower η is on the safe side).
pub fn eta(psi: f64) -> f64 {
    let p = psi.clamp(-0.5, 1.0);
    1.75 - 1.05 * p + 0.3 * p * p
}

/// ω of a moment distribution (5.1.9.1, Figure 16): 0.6 + 0.4 ψ ≥ 0.4.
pub fn omega(psi: f64) -> f64 {
    (0.6 + 0.4 * psi.clamp(-1.0, 1.0)).max(0.4)
}

/// The class limits of an element in uniform compression, b/t over ε:
/// classes 1, 2, 3 (Tables 5a, 5b); a tube's D/t over ε².
#[derive(Debug, Clone, Copy, PartialEq)]
enum Element {
    /// Supported at both edges (Table 5a, compression: 33, 38, 42).
    Internal(f64),
    /// Free at one edge (Table 5b, compression: 9, 10, 14).
    Outstand(f64),
    /// A tube's D/t (Table 5a, tubes: 50, 70, 90 ε²).
    Tube(f64),
}

impl Element {
    /// The section class of this element at yield strength `fy`.
    fn class(self, fy: f64) -> u8 {
        let e = epsilon(fy);
        let (x, limits) = match self {
            Element::Internal(b_t) => (b_t / e, [33.0, 38.0, 42.0]),
            Element::Outstand(b_t) => (b_t / e, [9.0, 10.0, 14.0]),
            Element::Tube(d_t) => (d_t / (e * e), [50.0, 70.0, 90.0]),
        };
        limits
            .iter()
            .position(|l| x <= *l)
            .map_or(4, |k| k as u8 + 1)
    }
}

/// The compressed elements of a profile for its class (b as Tables 5a,
/// 5b measure it, on the safe side where they do not say: an angle's leg
/// and an I section's half flange taken whole) and the plates of its
/// effective section (b from the theoretical lines, 4.5.3.3), m.
struct Plates {
    /// For the class: the elements.
    class: Vec<Element>,
    /// For the effective section: (b, t, k, count).
    effective: Vec<(f64, f64, f64, f64)>,
}

fn plates(shape: &ohl_sections::Shape) -> Option<Plates> {
    use ohl_sections::Shape as S;
    Some(match *shape {
        S::EqualAngle { h, t, .. } => Plates {
            class: vec![Element::Outstand(h / t)],
            effective: vec![(h - t / 2.0, t, K_OUTSTAND, 2.0)],
        },
        S::UnequalAngle { h, b, t, .. } => Plates {
            class: vec![Element::Outstand(h.max(b) / t)],
            effective: vec![
                (h - t / 2.0, t, K_OUTSTAND, 1.0),
                (b - t / 2.0, t, K_OUTSTAND, 1.0),
            ],
        },
        S::DoubleAngle { h, b, t, .. } | S::StarAngle { h, b, t, .. } => Plates {
            class: vec![Element::Outstand(h.max(b) / t)],
            effective: vec![
                (h - t / 2.0, t, K_OUTSTAND, 2.0),
                (b - t / 2.0, t, K_OUTSTAND, 2.0),
            ],
        },
        S::CrossAngle { h, t, .. } => Plates {
            class: vec![Element::Outstand(h / t)],
            effective: vec![(h - t / 2.0, t, K_OUTSTAND, 8.0)],
        },
        S::ISection { h, b, tw, tf, .. } => Plates {
            class: vec![
                Element::Outstand(b / 2.0 / tf),
                Element::Internal((h - 2.0 * tf) / tw),
            ],
            effective: vec![
                (b / 2.0, tf, K_OUTSTAND, 4.0),
                (h - tf, tw, K_INTERNAL, 1.0),
            ],
        },
        S::Channel { h, b, tw, tf, .. } => Plates {
            class: vec![
                Element::Outstand(b / tf),
                Element::Internal((h - 2.0 * tf) / tw),
            ],
            effective: vec![
                (b - tw / 2.0, tf, K_OUTSTAND, 2.0),
                (h - tf, tw, K_INTERNAL, 1.0),
            ],
        },
        S::RectangularHollow { h, b, t } => Plates {
            // Table 5a: b less 1.5 t at each side.
            class: vec![Element::Internal((h.max(b) - 3.0 * t) / t)],
            effective: vec![(h - t, t, K_INTERNAL, 2.0), (b - t, t, K_INTERNAL, 2.0)],
        },
        S::CircularHollow { d, t } => Plates {
            class: vec![Element::Tube(d / t)],
            effective: Vec::new(),
        },
        S::PolygonalHollow { sides, d, t } => {
            let n = f64::from(sides.max(3));
            let flat = (d - t) * (std::f64::consts::PI / n).tan();
            Plates {
                class: vec![Element::Internal(flat / t)],
                effective: vec![(flat, t, K_INTERNAL, n)],
            }
        }
        S::RoundBar { .. } => Plates {
            class: Vec::new(),
            effective: Vec::new(),
        },
        S::General { .. } => return None,
    })
}

/// A section's class and effective area, m², and the least ρ of its
/// plates (for W_eff ≈ ρ_min W_el, on the safe side); class 4 of a tube:
/// no rule (none).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Classed {
    class: u8,
    a_eff: f64,
    rho_min: f64,
}

fn classify(s: &Section, fy: f64) -> Option<Classed> {
    let p = s.profile.as_ref()?;
    let pl = plates(&p.shape)?;
    let class = pl.class.iter().map(|e| e.class(fy)).max().unwrap_or(1);
    if class < 4 {
        return Some(Classed {
            class,
            a_eff: s.area,
            rho_min: 1.0,
        });
    }
    if pl.effective.is_empty() {
        return None;
    }
    let mut a_eff = s.area;
    let mut rho_min: f64 = 1.0;
    for (b, t, k, n) in pl.effective {
        let r = rho(b / t, k, fy);
        rho_min = rho_min.min(r);
        a_eff -= n * (1.0 - r) * b * t;
    }
    Some(Classed {
        class,
        a_eff: a_eff.max(0.0),
        rho_min,
    })
}

/// The buckling curve of a section about its strong (`strong`) or weak
/// axis (Figure 7), and what it assumed: rolled and welded I sections
/// not told apart (b/c up to t_f 40 mm, else c/d: the welded ones'),
/// hollow sections as cold-formed (c), a `general` profile d.
fn curve(s: &Section, strong: bool) -> (Curve, Option<&'static str>) {
    use ohl_sections::Shape as S;
    match s.profile.as_ref().map(|p| &p.shape) {
        Some(S::ISection { tf, .. }) => {
            let thick = *tf > 0.040;
            let c = match (strong, thick) {
                (true, false) => Curve::B,
                (false, false) | (true, true) => Curve::C,
                (false, true) => Curve::D,
            };
            (c, Some("I section as welded (curves b/c, c/d)"))
        }
        Some(
            S::RectangularHollow { .. } | S::CircularHollow { .. } | S::PolygonalHollow { .. },
        ) => (Curve::C, Some("hollow section as cold-formed (curve c)")),
        Some(S::General { .. }) | None => (Curve::D, Some("curve d (profile not known)")),
        _ => (Curve::C, None),
    }
}

/// One axis of the stability check (formulas 50, 62, 63).
#[derive(Debug, Clone, PartialEq)]
pub struct Axis {
    /// Its name ("y", "z", "u", "v").
    pub name: &'static str,
    /// The elastic critical load N_cr = π² E I/L_K², N.
    pub n_cr: f64,
    /// The bending resistance M_Rd = f_y W/γ_M1, N·m (W plastic for
    /// classes 1 and 2 where the profile gives it, elastic for 3, ρ_min
    /// W_el for 4).
    pub m_rd: f64,
    /// Its section modulus W (as in `m_rd`), m³.
    pub w: f64,
    /// Its elastic section modulus (gross), m³ (λ̄_D = √(W f_y/M_cr), M_cr
    /// = W_el σ_cr,D).
    pub w_el: f64,
    /// The eccentricity of the axial force for bending about this axis,
    /// m (N·e; an angle bolted by one leg, the effective section's shift).
    pub e: f64,
    /// The equivalent bow w_0 (Figure 3), m.
    pub w0: f64,
    /// Lateral-torsional buckling about this (major) axis: σ_cr,D at η =
    /// 1, Pa, and α_D (Annex B, 4.5.2.3).
    pub lt: Option<(f64, f64)>,
}

/// What the stability check of a member under N and bending needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Stability {
    /// Class 4: formulas (62)/(63), else (50).
    pub class4: bool,
    /// The axes: major (strong) first.
    pub axes: [Axis; 2],
    /// N_K,Rd (least over the axes), N.
    pub n_k: f64,
    /// The tension resistance of the member (5.1.2), N.
    pub n_t: f64,
    /// The effective area (class 4), m².
    pub a_eff: f64,
    /// f_y, Pa.
    pub fy: f64,
    /// The bending moments are the axial force's eccentricity (an angle
    /// bolted by one leg: uniform, ω = 1), not the frame's end moments.
    pub eccentric: bool,
}

impl Stability {
    /// The usage under axial force `n` (tension positive) and the frame's
    /// end moments `[[M_i, M_j]; 2]` about the two axes as internal
    /// moments with signs (zero for an axial member), with the load
    /// case's strength factor: formula (50) in compression (classes 1–3),
    /// (62)/(63) for class 4, the linear sum N/N_Rd + ΣM/M_Rd in tension.
    pub fn usage(&self, n: f64, moments: [[f64; 2]; 2], strength: f64) -> (f64, String) {
        let kn = |x: f64| x / 1000.0;
        let comp = (-n).max(0.0);
        let tens = n.max(0.0);
        // The largest moment and ψ per axis.
        let ends = |a: usize| {
            let [mi, mj] = moments[a];
            let (big, small) = if mi.abs() >= mj.abs() {
                (mi, mj)
            } else {
                (mj, mi)
            };
            let psi = if big == 0.0 { 1.0 } else { small / big };
            (big.abs(), psi)
        };
        if tens > 0.0 || comp == 0.0 {
            let mut u = tens / (strength * self.n_t);
            let mut words = vec![format!(
                "tension {:.1} kN of {:.1} kN",
                kn(tens),
                kn(self.n_t)
            )];
            for (a, ax) in self.axes.iter().enumerate() {
                let (m, _) = ends(a);
                if m > 0.0 && ax.m_rd > 0.0 {
                    u += m / (strength * ax.m_rd);
                    words.push(format!(
                        "M{} {:.2} kN m of {:.2}",
                        ax.name,
                        kn(m),
                        kn(ax.m_rd)
                    ));
                }
            }
            return (u, words.join(" + "));
        }
        if self.class4 {
            // (62), (63): σ = N/A_eff + amplified about one axis.
            let fyd = strength * self.fy / GAMMA_M1;
            let mut worst = (0.0f64, String::new());
            for k in 0..2 {
                let mut sigma = comp / self.a_eff;
                for (a, ax) in self.axes.iter().enumerate() {
                    let (m, _) = ends(a);
                    let m = m + comp * (ax.e.abs() + ax.w0);
                    let amp = if a == k {
                        if comp >= ax.n_cr {
                            return (f64::INFINITY, format!("N at N_cr,{} (class 4)", ax.name));
                        }
                        1.0 / (1.0 - comp / ax.n_cr)
                    } else {
                        1.0
                    };
                    if ax.w > 0.0 {
                        sigma += amp * m / ax.w;
                    }
                }
                let u = sigma / fyd;
                if u > worst.0 {
                    worst = (
                        u,
                        format!(
                            "compression {:.1} kN, class 4: σ {:.0} MPa of {:.0} (formula {})",
                            kn(comp),
                            sigma / 1e6,
                            fyd / 1e6,
                            if k == 0 { 62 } else { 63 }
                        ),
                    );
                }
            }
            return worst;
        }
        // (50).
        let mut u = comp / (strength * self.n_k);
        let mut words = vec![format!(
            "compression {:.1} kN of {:.1} kN",
            kn(comp),
            kn(self.n_k)
        )];
        for (a, ax) in self.axes.iter().enumerate() {
            let (m, psi) = ends(a);
            let (m, psi) = if self.eccentric {
                (comp * ax.e.abs(), 1.0)
            } else {
                (m, psi)
            };
            if m == 0.0 {
                continue;
            }
            if comp >= ax.n_cr {
                return (f64::INFINITY, format!("N at N_cr,{} (formula 50)", ax.name));
            }
            let mrd = match ax.lt {
                Some((sigma0, alpha)) => {
                    let sigma = eta(psi) * sigma0;
                    let lb = (self.fy / sigma * ax.w / ax.w_el.max(f64::MIN_POSITIVE)).sqrt();
                    chi_lt(lb, alpha) * ax.m_rd
                }
                None => ax.m_rd,
            };
            let term = omega(psi) / (1.0 - comp / ax.n_cr) * m / (strength * mrd);
            u += term;
            words.push(format!(
                "M{} {:.2} kN m of {:.2}{}",
                ax.name,
                kn(m),
                kn(mrd),
                if ax.lt.is_some() && mrd < ax.m_rd {
                    " (lateral-torsional)"
                } else {
                    ""
                }
            ));
        }
        let words = if words.len() > 1 {
            format!("{} (formula 50)", words.join(" + "))
        } else {
            words.remove(0)
        };
        (u, words)
    }
}

/// The slenderness limit λ_K of a member class (4.5.1.7): bracing and
/// secondary members 250, main load-carrying members 200 (a tower's
/// legs and loaded bracing: their loads are computed); the members
/// without a class 200.
fn class_limit(class: Option<MemberClass>) -> (f64, &'static str) {
    match class {
        Some(MemberClass::Redundant) => (250.0, "secondary"),
        _ => (200.0, "load-carrying"),
    }
}

/// The holes of standard bolts (Table 17): (bolt, hole), m.
const HOLES: [(f64, f64); 7] = [
    (0.010, 0.012),
    (0.012, 0.014),
    (0.016, 0.018),
    (0.020, 0.022),
    (0.024, 0.026),
    (0.027, 0.030),
    (0.030, 0.033),
];

/// The minimum distances of Table 17 by bolt: (bolt, p_1 = p_2, e_1, e_2), m.
const MINIMUMS: [(f64, f64, f64, f64); 7] = [
    (0.010, 0.030, 0.015, 0.012),
    (0.012, 0.035, 0.020, 0.015),
    (0.016, 0.040, 0.025, 0.020),
    (0.020, 0.045, 0.030, 0.025),
    (0.024, 0.055, 0.035, 0.030),
    (0.027, 0.065, 0.040, 0.035),
    (0.030, 0.070, 0.045, 0.040),
];

fn by_size<T: Copy>(table: &[(f64, T)], d: f64) -> Option<T> {
    table
        .iter()
        .find(|(size, _)| (size - d).abs() < 1e-5)
        .map(|r| r.1)
}

/// The minimum end distance e_1, spacing p_1 and edge distance e_2 of a
/// bolt of diameter `d` (Table 17, M10 to M30), m; none for another size.
pub fn bolt_minimums(d: f64) -> Option<[f64; 3]> {
    let rows: Vec<(f64, [f64; 3])> = MINIMUMS.iter().map(|r| (r.0, [r.2, r.1, r.3])).collect();
    by_size(&rows, d)
}

/// The standard hole of a bolt (Table 17), m.
pub fn standard_hole(d: f64) -> Option<f64> {
    by_size(&HOLES, d)
}

/// The shear factor of a bolt grade with the threads in the shear plane
/// (Table 16): 0.6 for 4.6, 5.6 and 8.8 (73), 0.5 for 4.8, 5.8, 6.8 and
/// 10.9 (74); another grade or none: 0.5 (the lower; said by the caller).
fn shear_factor(grade: Option<crate::design::Grade>) -> (f64, bool) {
    match grade.map(|g| (g.major, g.minor)) {
        Some((4, 6) | (5, 6) | (8, 8)) => (0.6, true),
        Some((4, 8) | (5, 8) | (6, 8) | (10, 9)) => (0.5, true),
        _ => (0.5, false),
    }
}

/// The root area over the gross area when nothing better is known (as
/// for ASCE 10-97: on the safe side against the tensile stress area A_s
/// of Table 16).
const ROOT_AREA: f64 = 0.6;

/// One end's connection as the check takes it.
struct EndCheck {
    /// The bolts' shear, N.
    shear: Option<f64>,
    /// Bearing, N.
    bearing: Option<f64>,
    /// The tension capacity at this end and what governs it.
    tension: (f64, String),
    /// What the connection assumed or could not check.
    notes: Vec<String>,
    /// Conditions of a formula not met.
    limits: Vec<String>,
}

/// The connection's capacities at one end.
#[allow(clippy::too_many_arguments)]
fn end_check(
    d: &MemberDesign,
    end: Option<&EndDesign>,
    s: &Section,
    thickness: Option<f64>,
    fy: f64,
    a: f64,
    angle: Option<(f64, f64)>,
) -> EndCheck {
    let mut notes: Vec<String> = Vec::new();
    let mut limits: Vec<String> = Vec::new();
    let plies = f64::from(d.plies.unwrap_or(1));
    let gross = (fy * a / GAMMA_M1, "gross section (38)".to_owned());
    let bolts = crate::asce10::end_bolts(d, end, &mut notes);
    let Some((b, shank, threads_in)) = bolts else {
        if notes.is_empty() {
            notes.push("no bolts given: connection not checked".into());
        }
        return EndCheck {
            shear: None,
            bearing: None,
            tension: gross,
            notes,
            limits,
        };
    };
    let hole = match b.hole.or_else(|| standard_hole(b.diameter)) {
        Some(h) => {
            if b.hole.is_none() {
                notes.push("standard hole (Table 17)".into());
            }
            h
        }
        None => {
            notes.push("hole: the bolt + 3 mm".into());
            b.diameter + 0.003
        }
    };
    // Shear (Table 16) and bearing (76).
    let mut shear = None;
    let mut bearing = None;
    if b.count == 0 {
        notes.push("no end bolts: connection not checked".into());
    } else {
        let area = std::f64::consts::PI * b.diameter.powi(2) / 4.0;
        let out = threads_in == Some(false);
        let per_plane = match (shank.filter(|_| out), b.shear_capacity, b.ultimate) {
            (Some(v), _, _) | (None, Some(v), _) => {
                if out && shank.is_none() {
                    notes.push("bolt shear as with the threads in the shear plane".into());
                }
                Some(v / GAMMA_M2)
            }
            (None, None, Some(fub)) => {
                let (k, known) = shear_factor(b.grade);
                if !known {
                    notes.push("bolt grade not known: 0.5 f_ub (74)".into());
                }
                if threads_in.is_none() {
                    notes.push("bolt threads in the shear plane".into());
                }
                notes.push("tensile stress area 0.6 of the gross".into());
                Some(k * fub * ROOT_AREA * area / GAMMA_M2)
            }
            _ => {
                notes.push("bolt shear not checked (no capacity)".into());
                None
            }
        };
        shear = per_plane.map(|v| f64::from(b.count) * f64::from(b.shear_planes) * v);
        let lines = b.holes.unwrap_or(1).max(1);
        let per_line = b.count.div_ceil(lines).max(1);
        let e1 = end
            .and_then(|e| e.end_distance)
            .or(d.end_distance)
            .or_else(|| {
                let m = bolt_minimums(b.diameter)?[0];
                notes.push("end distance the minimum of Table 17".into());
                Some(m)
            });
        let p1 = end.and_then(|e| e.pitch).or(d.spacing);
        let e2 = end.and_then(|e| e.edge_distance).or(d.edge_distance);
        match (s.ultimate, thickness, e1) {
            (Some(fu), Some(t), Some(e1)) => {
                // An inner bolt with p_1 < e_1 + d_0/2: p_1 − d_0/2 (Table 16).
                let mut e = e1;
                if per_line > 1 {
                    match p1 {
                        Some(p) if p < e1 + hole / 2.0 => e = p - hole / 2.0,
                        Some(_) => {}
                        None => {
                            notes.push("pitch not known: bearing by e_1 alone".into());
                        }
                    }
                }
                let one = (0.85 * e / hole * fu * b.diameter * t).min(2.4 * fu * b.diameter * t)
                    / GAMMA_M2;
                bearing = Some(f64::from(b.count) * one * plies);
                if let Some(e2) = e2
                    && e2 < hole
                {
                    limits.push(format!(
                        "e_2 {:.0} mm below d_0 {:.0} mm (Table 16)",
                        e2 * 1e3,
                        hole * 1e3
                    ));
                }
            }
            (None, _, _) => notes.push("bearing not checked (no ultimate strength)".into()),
            (_, None, _) => notes.push("bearing not checked (no thickness)".into()),
            (_, _, None) => notes.push("bearing not checked (no end distance)".into()),
        }
    }
    // Tension: net section (39), an angle bolted by one leg 6.2.4.4.
    let leg = end.and_then(|e| e.leg).or(d.connected_leg);
    let mut tension = gross;
    match (thickness, s.ultimate) {
        (Some(t), Some(fu)) => {
            let holes = f64::from(b.holes.unwrap_or_else(|| {
                notes.push("one hole per section".into());
                1
            }));
            let net = 0.9 * fu * (a - holes * hole * t).max(0.0) / GAMMA_M2;
            if net < tension.0 {
                tension = (net, "net section (39)".to_owned());
            }
            let one_leg = leg.is_none_or(ConnectedLeg::one_leg);
            if let (true, Some((long, short)), true) = (one_leg, angle, b.count > 0) {
                if leg.is_none() {
                    notes.push(if long > short {
                        "one leg bolted, the short one".into()
                    } else {
                        "one leg bolted".into()
                    });
                }
                let width = if leg == Some(ConnectedLeg::Long) || (leg.is_none() && long == short) {
                    long
                } else {
                    short
                };
                match angle_connection(d, end, &b, hole, t, width, fy, fu) {
                    Ok((v, what, said)) => {
                        if let Some(w) = said {
                            notes.push(w.into());
                        }
                        if v < tension.0 {
                            tension = (v, what.to_owned());
                        }
                    }
                    Err(why) => notes.push(format!("6.2.4.4 not checked ({why})")),
                }
            }
        }
        (None, _) => notes.push("net section not checked (no thickness)".into()),
        (_, None) => notes.push("net section not checked (no ultimate strength)".into()),
    }
    for (v, what) in [(shear, "bolt shear"), (bearing, "bearing")] {
        if let Some(v) = v
            && v < tension.0
        {
            tension = (v, what.to_owned());
        }
    }
    EndCheck {
        shear,
        bearing,
        tension,
        notes,
        limits,
    }
}

/// An angle bolted by one leg in tension (6.2.4.4, Figure 30): one bolt
/// (84) 0.9 f_u (2 e_2 − d_0) t/γ_M2; several on a line (83) (0.9 f_u
/// A_t plus f_y/√3 A_v)/γ_M2 with A_t = (e_2 − d_0/2) t, A_v = ((n − 1) p + e_1)
/// t, e_2 from the line to the leg's toe. Several gauge lines (6.2.4.6:
/// "une attention particulière"): the outer line alone carrying the force
/// (on the safe side, said).
#[allow(clippy::too_many_arguments)]
fn angle_connection(
    d: &MemberDesign,
    end: Option<&EndDesign>,
    b: &Bolts,
    hole: f64,
    t: f64,
    width: f64,
    fy: f64,
    fu: f64,
) -> Result<(f64, &'static str, Option<&'static str>), &'static str> {
    let lines = b.holes.unwrap_or(1).max(1);
    let n = b.count.div_ceil(lines).max(1);
    let gauges = end.map(|e| e.gauges.as_slice()).unwrap_or(&[]);
    let toe = match gauges.iter().copied().reduce(f64::max) {
        Some(g) => width - g,
        None => end
            .and_then(|e| e.edge_distance)
            .or(d.edge_distance)
            .ok_or("no edge distance")?,
    };
    let said = (lines > 1).then_some("6.2.4.4: the outer gauge line alone");
    if n == 1 {
        let v = 0.9 * fu * (2.0 * toe - hole).max(0.0) * t / GAMMA_M2;
        return Ok((v, "one bolt (84)", said));
    }
    let e1 = end
        .and_then(|e| e.end_distance)
        .or(d.end_distance)
        .ok_or("no end distance")?;
    let p = end.and_then(|e| e.pitch).or(d.spacing).ok_or("no pitch")?;
    let at = (toe - hole / 2.0).max(0.0) * t;
    let av = (f64::from(n - 1) * p + e1) * t;
    let v = (0.9 * fu * at + fy / 3f64.sqrt() * av) / GAMMA_M2;
    Ok((v, "shear and tearing (83)", said))
}

/// The SIA 263 capacities of member `idx`, and what the stability check
/// with bending needs ([`Stability`]). `partner_in_tension` as for
/// [`crate::asce10::capacity`].
pub fn capacity(
    model: &Model,
    idx: usize,
    partner_in_tension: Option<bool>,
) -> (Capacity, Option<Stability>) {
    let m: &Member = &model.members[idx];
    let s = &model.sections[m.section];
    let d = &m.design;
    let mut out = Capacity::default();
    let Some(fy) = s.yield_strength else {
        return (out, None);
    };
    let e_mod = s.e;
    let a = s.area;
    let mut assumed: Vec<String> = Vec::new();
    let mut limits: Vec<String> = Vec::new();
    let props = s.profile.as_ref().map(|p| p.properties());
    let shape = s.profile.as_ref().map(|p| p.shape.clone());
    let single_angle = matches!(
        shape,
        Some(ohl_sections::Shape::EqualAngle { .. } | ohl_sections::Shape::UnequalAngle { .. })
    );
    let crossed = matches!(shape, Some(ohl_sections::Shape::CrossAngle { .. }));
    // Buckling lengths (5.4.1.3): between the joints held.
    let length = m.lk.unwrap_or_else(|| model.length(m));
    let ratios = match d.length_ratios {
        Some(r) => r,
        None => {
            if m.lk.is_none() && m.k == 1.0 {
                assumed.push("buckling lengths L (5.4.1.3)".into());
            }
            [if m.lk.is_some() { 1.0 } else { m.k }; 3]
        }
    };
    let ends = member_ends::end_conditions(model, idx);
    let eccentric = ends.eccentric.iter().any(|c| c.value != Some(false));
    if ends
        .eccentric
        .iter()
        .any(|c| c.source == member_ends::Source::Assumed)
        && single_angle
    {
        assumed.push("connected eccentrically".into());
    }
    // Class and effective area.
    let classed = match classify(s, fy) {
        Some(c) => c,
        None => {
            if s.profile.is_some()
                && matches!(shape, Some(ohl_sections::Shape::CircularHollow { .. }))
            {
                limits.push("tube of class 4 (D/t above 90 ε²): outside SIA 263".into());
                out.limit = Some(limits.join("; "));
                return (out, None);
            }
            assumed.push("section class 3 (no library profile)".into());
            Classed {
                class: 3,
                a_eff: a,
                rho_min: 1.0,
            }
        }
    };
    let class4 = classed.class == 4;
    // The axes: (name, I, L_K, curve).
    let (strong_curve, said) = curve(s, true);
    let (weak_curve, _) = curve(s, false);
    if let Some(w) = said {
        assumed.push(w.into());
    }
    let mut axes: Vec<(&'static str, f64, f64, Curve)> = Vec::new();
    match (&props, single_angle || crossed) {
        (Some(q), true) => {
            let lv = ratios[2] * length * if eccentric || crossed { 0.9 } else { 1.0 };
            axes.push(("u", q.iu, ratios[0].max(ratios[1]) * length, strong_curve));
            axes.push(("v", q.iv, lv, weak_curve));
        }
        _ => {
            let (strong_y, ia, ib) = if s.iy >= s.iz {
                (true, s.iy, s.iz)
            } else {
                (false, s.iz, s.iy)
            };
            let (la, lb) = if strong_y {
                (ratios[0] * length, ratios[1] * length)
            } else {
                (ratios[1] * length, ratios[0] * length)
            };
            let (na, nb) = if strong_y { ("y", "z") } else { ("z", "y") };
            axes.push((na, ia, la, strong_curve));
            axes.push((nb, ib, lb, weak_curve));
            if !single_angle && s.i_min > 0.0 && s.i_min < ib * 0.999 {
                axes.push(("v", s.i_min, ratios[2] * length, weak_curve));
            }
        }
    }
    // A crossing diagonal whose partner is not in tension: its whole
    // length out of the plane (as for ASCE 10-97).
    if d.class == Some(MemberClass::Crossing) && partner_in_tension != Some(true) {
        let geometric = s.iy.min(s.iz);
        if geometric > 0.0 {
            axes.push(("out of plane", geometric, model.length(m), weak_curve));
            assumed.push(if partner_in_tension.is_some() {
                "crossing partner not in tension: unbraced out of plane".into()
            } else {
                "crossing diagonal without a partner: unbraced out of plane".into()
            });
        }
    }
    // Flexural buckling (4.5.1), λ̄_K by 5.3.8 for class 4.
    let area_factor = (classed.a_eff / a).sqrt();
    let lambda_e = std::f64::consts::PI * (e_mod / fy).sqrt();
    let mut n_k = f64::INFINITY;
    let mut governs = String::new();
    let mut lambda_max: f64 = 0.0;
    for &(name, i, lk, c) in &axes {
        if i <= 0.0 || lk <= 0.0 {
            continue;
        }
        let lk_i = lk / (i / a).sqrt();
        lambda_max = lambda_max.max(lk_i);
        let lb = lk_i / lambda_e * area_factor;
        let n = chi(lb, c.alpha()) * fy * classed.a_eff / GAMMA_M1;
        if n < n_k {
            n_k = n;
            governs = format!("λ_K {lk_i:.0} about {name}, curve {}", c.letter());
        }
    }
    // Torsional and torsional-flexural buckling by elastic theory.
    if let Some(r) = crate::asce10::torsional_slenderness(s, length, ratios) {
        match r {
            Ok((l, what)) => {
                let lb = l / lambda_e * area_factor;
                let n = chi(lb, weak_curve.alpha()) * fy * classed.a_eff / GAMMA_M1;
                if n < n_k {
                    n_k = n;
                    let mode = if what.contains("flexural") {
                        "torsional-flexural"
                    } else {
                        "torsional"
                    };
                    governs = format!("λ {l:.0} {mode} (elastic), curve {}", weak_curve.letter());
                    assumed.push("free to warp".into());
                }
            }
            Err(()) => assumed.push("torsional buckling not checked (general profile)".into()),
        }
    }
    if !n_k.is_finite() {
        n_k = fy * classed.a_eff / GAMMA_M1;
        governs = "no second moment: squash".into();
    }
    if class4 {
        governs.push_str(&format!(", class 4: A_eff {:.0} mm²", classed.a_eff * 1e6));
    } else {
        governs.push_str(&format!(", class {}", classed.class));
    }
    let (limit, kind) = class_limit(d.class);
    if lambda_max > limit {
        limits.push(format!(
            "λ_K {lambda_max:.0} above {limit:.0} ({kind}, 4.5.1.7)"
        ));
    }
    out.kl_r = Some(lambda_max);
    // The connection at each end.
    let angle = match shape {
        Some(ohl_sections::Shape::EqualAngle { h, .. }) => Some((h, h)),
        Some(ohl_sections::Shape::UnequalAngle { h, b, .. }) => Some((h.max(b), h.min(b))),
        _ => None,
    };
    let leg_t = match shape {
        Some(
            ohl_sections::Shape::EqualAngle { t, .. }
            | ohl_sections::Shape::UnequalAngle { t, .. }
            | ohl_sections::Shape::DoubleAngle { t, .. }
            | ohl_sections::Shape::StarAngle { t, .. }
            | ohl_sections::Shape::CrossAngle { t, .. },
        ) => Some(t),
        _ => None,
    };
    let thickness = d.thickness.or(leg_t);
    let checks = [0, 1].map(|end| end_check(d, d.ends[end].as_ref(), s, thickness, fy, a, angle));
    let at = |end: usize| model.nodes[if end == 0 { m.i } else { m.j }].id.as_str();
    let alike = (checks[0].shear, checks[0].bearing, &checks[0].tension)
        == (checks[1].shear, checks[1].bearing, &checks[1].tension);
    let name = |what: &str, end: usize| {
        if alike {
            what.to_owned()
        } else {
            format!("{what} at {}", at(end))
        }
    };
    let mut comp = (n_k, governs.clone());
    for (end, c) in checks.iter().enumerate() {
        for (v, what) in [(c.shear, "bolt shear"), (c.bearing, "bearing")] {
            if let Some(v) = v
                && v < comp.0
            {
                comp = (v, name(what, end));
            }
        }
    }
    // Notes and limits, named by their end when the other end has not
    // the same.
    let merge = |pick: fn(&EndCheck) -> &Vec<String>| -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (end, c) in checks.iter().enumerate() {
            for n in pick(c) {
                let n = if pick(&checks[1 - end]).contains(n) {
                    n.clone()
                } else {
                    format!("{n} at {}", at(end))
                };
                if !out.contains(&n) {
                    out.push(n);
                }
            }
        }
        out
    };
    let notes = merge(|c| &c.notes);
    limits.extend(merge(|c| &c.limits));
    let mut tension = (f64::INFINITY, String::new());
    for (end, c) in checks.iter().enumerate() {
        if c.tension.0 < tension.0 {
            tension = (c.tension.0, name(&c.tension.1, end));
        }
    }
    if let Some(t) = s.tension {
        tension = (t, "allowable".to_owned());
    }
    // The stability check with bending: the axes' N_cr, M_Rd, the
    // eccentricity of an angle bolted by one leg, the bow.
    let stability = props.as_ref().map(|q| {
        let pair = if single_angle || crossed {
            [("u", q.iu, q.wel_u), ("v", q.iv, q.wel_v)]
        } else if s.iy >= s.iz {
            [
                (
                    "y",
                    s.iy,
                    if classed.class <= 2 {
                        q.wpl_y.or(s.wy)
                    } else {
                        s.wy
                    },
                ),
                (
                    "z",
                    s.iz,
                    if classed.class <= 2 {
                        q.wpl_z.or(s.wz)
                    } else {
                        s.wz
                    },
                ),
            ]
        } else {
            [
                (
                    "z",
                    s.iz,
                    if classed.class <= 2 {
                        q.wpl_z.or(s.wz)
                    } else {
                        s.wz
                    },
                ),
                (
                    "y",
                    s.iy,
                    if classed.class <= 2 {
                        q.wpl_y.or(s.wy)
                    } else {
                        s.wy
                    },
                ),
            ]
        };
        // Lengths as the flexural check took them.
        let lk_of = |n: &str| axes.iter().find(|a| a.0 == n).map_or(length, |a| a.2);
        // The eccentricity of an angle bolted by one leg (5.4.1.2): the
        // force on the back of the bolted leg at its gauge line.
        let mut e = [0.0f64; 2];
        let mut ecc_used = false;
        if single_angle && eccentric {
            if let Some(Ok((dy, dz, said))) = angle_offset(model, idx, q) {
                let th = q.principal_angle;
                let eu = dy * th.cos() + dz * th.sin();
                let ev = -dy * th.sin() + dz * th.cos();
                // About u: the offset along v; about v: along u.
                e = [ev, eu];
                ecc_used = true;
                if let Some(w) = said {
                    assumed.push(w);
                }
            } else {
                assumed.push("eccentricity of the connection not known: left out".into());
            }
        }
        // Class 4: the effective section's moduli and centroid shift.
        let mut w_factor = classed.rho_min;
        if class4 && let Some((shift, w)) = effective_angle(s, fy, q) {
            let th = q.principal_angle;
            e[0] += -shift[0] * th.sin() + shift[1] * th.cos();
            e[1] += shift[0] * th.cos() + shift[1] * th.sin();
            w_factor = w;
        }
        let ltb = lateral_torsional(s, q, pair[1].1, ratios, length, single_angle);
        if let Some(w) = ltb.1 {
            assumed.push(w.into());
        }
        let elastic = |name: &str| match name {
            "u" => q.wel_u,
            "v" => q.wel_v,
            "y" => s.wy.or(q.wel_y),
            _ => s.wz.or(q.wel_z),
        };
        let axis = |k: usize, lt: Option<(f64, f64)>| {
            let (name, i, w) = pair[k];
            let w_el = elastic(name).unwrap_or(0.0);
            let w = w.unwrap_or(0.0) * if class4 { w_factor } else { 1.0 };
            let lk = lk_of(name);
            let c = if k == 0 { strong_curve } else { weak_curve };
            Axis {
                name,
                n_cr: if lk > 0.0 {
                    std::f64::consts::PI.powi(2) * e_mod * i / (lk * lk)
                } else {
                    f64::INFINITY
                },
                m_rd: fy * w / GAMMA_M1,
                w,
                w_el,
                e: e[k],
                w0: c.bow() * lk_of(name).max(0.0)
                    / if name == "v" && (eccentric || crossed) {
                        0.9
                    } else {
                        1.0
                    },
                lt,
            }
        };
        Stability {
            class4,
            axes: [axis(0, ltb.0), axis(1, None)],
            n_k,
            n_t: tension.0,
            a_eff: classed.a_eff,
            fy,
            eccentric: ecc_used,
        }
    });
    out.compression = Some(comp);
    out.tension = Some(tension);
    out.compression_assumed = assumed;
    out.compression_assumed.extend(notes.iter().cloned());
    out.tension_assumed = notes;
    if !limits.is_empty() {
        out.limit = Some(limits.join("; "));
    }
    if ends.connected.contains(&true) {
        for end in 0..2 {
            let c = &ends.eccentric[end];
            let word = match c.value {
                Some(true) => "eccentric",
                Some(false) => "concentric",
                None => "eccentric at one end",
            };
            out.joints.push(match &c.why {
                Some(w) => format!("{} {word} ({}: {w})", at(end), c.source.words()),
                None => format!("{} {word} ({})", at(end), c.source.words()),
            });
        }
    }
    (out, stability)
}

/// The offset (Δy, Δz) of the bolts' force on an angle bolted by one leg
/// from its centroid, in the profile's axes (heel at the origin, leg h
/// along z, leg b along y), m: on the back of the bolted leg, at its gauge
/// line(s) (their mean), else the farthest gauge Table 17 allows (the leg
/// less the minimum edge distance; on the safe side), else the middle of
/// the leg's flat; with what was assumed.
#[allow(clippy::type_complexity)]
fn angle_offset(
    model: &Model,
    idx: usize,
    q: &ohl_sections::Properties,
) -> Option<Result<(f64, f64, Option<String>), ()>> {
    let m = &model.members[idx];
    let s = &model.sections[m.section];
    let d = &m.design;
    let (h, b, t, r) = match s.profile.as_ref()?.shape {
        ohl_sections::Shape::EqualAngle { h, t, r } => (h, h, t, r),
        ohl_sections::Shape::UnequalAngle { h, b, t, r } => (h, b, t, r),
        _ => return None,
    };
    let end = d.ends.iter().flatten().next();
    let leg = end.and_then(|e| e.leg).or(d.connected_leg);
    // The leg along y (width b) or along z (width h).
    let along_y = match leg {
        Some(ConnectedLeg::Long) => b >= h,
        Some(ConnectedLeg::Short) => b < h || h == b,
        _ => b <= h,
    };
    let width = if along_y { b } else { h };
    let gauges: Vec<f64> = d
        .ends
        .iter()
        .flatten()
        .map(|e| e.gauges.clone())
        .find(|g| !g.is_empty())
        .unwrap_or_default();
    let mut said = None;
    let g = if gauges.is_empty() {
        let dia = end
            .and_then(|e| e.bolts.and_then(|b| b.diameter))
            .or(d.bolts.map(|b| b.diameter));
        match dia.and_then(bolt_minimums) {
            Some([_, _, e2]) => {
                said = Some("gauge the farthest Table 17 allows".to_owned());
                width - e2
            }
            None => {
                said = Some("gauge the middle of the leg's flat".to_owned());
                (t + r + width) / 2.0
            }
        }
    } else {
        gauges.iter().sum::<f64>() / gauges.len() as f64
    };
    let (cy, cz) = (q.centroid[0], q.centroid[1]);
    Some(Ok(if along_y {
        (g - cy, -cz, said)
    } else {
        (-cy, g - cz, said)
    }))
}

/// A class 4 angle's effective section (its legs b_eff = ρ b from the
/// heel, 4.5.3): the shift of its centroid (Δy, Δz), m, and its least
/// principal modulus over the gross one's.
fn effective_angle(s: &Section, fy: f64, q: &ohl_sections::Properties) -> Option<([f64; 2], f64)> {
    let (h, b, t, r) = match s.profile.as_ref()?.shape {
        ohl_sections::Shape::EqualAngle { h, t, r } => (h, h, t, r),
        ohl_sections::Shape::UnequalAngle { h, b, t, r } => (h, b, t, r),
        _ => return None,
    };
    let leg = |w: f64| t / 2.0 + rho((w - t / 2.0) / t, K_OUTSTAND, fy) * (w - t / 2.0);
    let (he, be) = (leg(h), leg(b));
    let shape = if (he - be).abs() < 1e-12 {
        ohl_sections::Shape::EqualAngle {
            h: he,
            t,
            r: r.min(he - t),
        }
    } else {
        ohl_sections::Shape::UnequalAngle {
            h: he,
            b: be,
            t,
            r: r.min(he.min(be) - t).max(0.0),
        }
    };
    let mut p = s.profile.clone()?;
    p.shape = shape;
    let e = p.properties();
    let ratio = |a: Option<f64>, b: Option<f64>| match (a, b) {
        (Some(a), Some(b)) if b > 0.0 => a / b,
        _ => 1.0,
    };
    let w = ratio(e.wel_u, q.wel_u)
        .min(ratio(e.wel_v, q.wel_v))
        .min(1.0);
    Some((
        [e.centroid[0] - q.centroid[0], e.centroid[1] - q.centroid[1]],
        w,
    ))
}

/// Lateral-torsional buckling about the major axis (Annex B; σ_cr,D at η =
/// 1 and α_D): I sections σ_cr,D = √(σ_Dv² + σ_Dw²) (91–93), the
/// compressed chord the flange and a third of the compressed half web;
/// channels, angles and other open sections σ_Dv only (approximate: Annex
/// B is for doubly symmetric sections; said); closed sections and bars
/// none. α_D 0.49 (welded; rolled and welded not told apart, on the safe
/// side).
fn lateral_torsional(
    s: &Section,
    q: &ohl_sections::Properties,
    i_minor: f64,
    ratios: [f64; 3],
    length: f64,
    angle: bool,
) -> (Option<(f64, f64)>, Option<&'static str>) {
    use ohl_sections::Shape as S;
    let Some(shape) = s.profile.as_ref().map(|p| &p.shape) else {
        return (None, None);
    };
    let k = q.it.unwrap_or(s.j);
    let w = if angle { q.wel_u } else { s.wy.or(q.wel_y) };
    let Some(w) = w.filter(|w| *w > 0.0) else {
        return (None, None);
    };
    let ld = if angle { ratios[2] } else { ratios[1] } * length;
    if ld <= 0.0 {
        return (None, None);
    }
    let alpha_d = 0.49;
    let g = s.g;
    let e = s.e;
    let sigma_v = std::f64::consts::PI / (ld * w) * (g * k * e * i_minor).sqrt();
    match *shape {
        S::ISection { h, b, tw, tf, .. } => {
            let hc = (h - 2.0 * tf) / 2.0;
            let af = b * tf + tw * hc / 3.0;
            let i_f = tf * b.powi(3) / 12.0 + (hc / 3.0) * tw.powi(3) / 12.0;
            let i2 = i_f / af;
            let sigma_w = std::f64::consts::PI.powi(2) * e * i2 / (ld * ld);
            (
                Some(((sigma_v * sigma_v + sigma_w * sigma_w).sqrt(), alpha_d)),
                Some("lateral-torsional: α_D 0.49 (as welded)"),
            )
        }
        S::Channel { .. } | S::EqualAngle { .. } | S::UnequalAngle { .. } => (
            Some((sigma_v, alpha_d)),
            Some("lateral-torsional by Annex B (92) alone: approximate for this profile"),
        ),
        _ => (None, None),
    }
}

/// The internal end moments `[[M_i, M_j]; 2]` of a frame member about the
/// stability check's two axes (major first), from its end forces in
/// local axes (`[.., M_y,i (4), M_z,i (5), .., M_y,j (10), M_z,j (11)]`).
pub fn frame_moments(st: &Stability, end_forces: &[f64; 12]) -> [[f64; 2]; 2] {
    let my = [-end_forces[4], end_forces[10]];
    let mz = [-end_forces[5], end_forces[11]];
    st.axes.each_ref().map(|a| match a.name {
        "y" => my,
        "z" => mz,
        _ => [0.0, 0.0],
    })
}

/// The usage of a steel pole's shaft section by SIA 263 (a round or
/// polygonal tube; owner 2026-11-03): axial force `p` (compression
/// positive), moment `m`, shear `v` and torsion `torsion` of the loads
/// above, yield `fy`, the load case's strength factor.
///
/// - class by Table 5a: a tube's D/t (50, 70, 90 ε²; class 4 has no rule
///   in SIA 263: said, no usage below infinity), a polygon's flats at
///   mid-thickness (D − t) tan(π/n) as internal elements (33, 38, 42 ε);
///   a class 4 polygon by its effective flats (ρ, k = 4: A_eff, and W_eff
///   ≈ ρ W, on the safe side);
/// - σ = N/A(_eff) + M/W(_eff) (5.2, 5.3.5: elastic, on the safe side
///   for classes 1 and 2), τ = 2V/A + T/(2 A_m t) (thin-walled, Bredt);
/// - the von Mises criterion (3.2.2.5) √(σ² + 3τ²) ≤ f_y/γ_M1 on the
///   compression and the tension side.
pub fn shaft_usage(
    s: &crate::asce48::Shaft,
    p: f64,
    m: f64,
    v: f64,
    torsion: f64,
    fy: f64,
    strength: f64,
) -> (f64, String) {
    let (a, i) = s.area_inertia();
    let w = i / s.reach();
    let e2 = epsilon(fy).powi(2);
    let (class, rho_eff, what) = match s.sides.filter(|n| *n <= 16) {
        None => {
            let d_t = s.d / s.t;
            let class = Element::Tube(d_t).class(fy);
            if class == 4 {
                return (
                    f64::INFINITY,
                    format!(
                        "D/t {d_t:.0} above 90 ε² = {:.0}: class 4 tube, outside SIA 263",
                        90.0 * e2
                    ),
                );
            }
            (class, 1.0, format!("D/t {d_t:.0}"))
        }
        Some(n) => {
            let n = f64::from(n);
            let flat = (s.d - s.t) * (std::f64::consts::PI / n).tan();
            let class = Element::Internal(flat / s.t).class(fy);
            let r = if class == 4 {
                rho(flat / s.t, K_INTERNAL, fy)
            } else {
                1.0
            };
            (class, r, format!("b/t {:.1}", flat / s.t))
        }
    };
    let (a_eff, w_eff) = if rho_eff < 1.0 {
        let n = f64::from(s.sides.unwrap_or(0));
        let flat = (s.d - s.t) * (std::f64::consts::PI / n.max(3.0)).tan();
        ((a - n * (1.0 - rho_eff) * flat * s.t).max(0.0), rho_eff * w)
    } else {
        (a, w)
    };
    let dm = s.d - s.t;
    let am = match s.sides {
        Some(n) => f64::from(n) * (dm / 2.0).powi(2) * (std::f64::consts::PI / f64::from(n)).tan(),
        None => std::f64::consts::PI / 4.0 * dm * dm,
    };
    let tau = 2.0 * v.abs() / a + torsion.abs() / (2.0 * am * s.t);
    let sb = m.abs() / w_eff;
    let fyd = strength * fy / GAMMA_M1;
    let mises = |sigma: f64| (sigma * sigma + 3.0 * tau * tau).sqrt();
    let comp = mises(p.max(0.0) / a_eff + sb);
    let tens = mises((-p).max(0.0) / a + sb);
    let (side, sigma) = if comp >= tens {
        ("compression", comp)
    } else {
        ("tension", tens)
    };
    (
        sigma / fyd,
        format!(
            "{side} side {:.0} MPa of f_y/γ_M1 {:.0} MPa (von Mises, 3.2.2.5), class {class} ({what}){}",
            sigma / 1e6,
            fyd / 1e6,
            if rho_eff < 1.0 {
                format!(", effective flats ρ {rho_eff:.2}")
            } else {
                String::new()
            }
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// χ_K at the curves' printed values: λ̄ 0.2 gives 1 on every curve;
    /// λ̄ 1.0 on curve c gives Φ 1.196 and χ 0.540 (formula 8 by hand).
    #[test]
    fn buckling_curves_by_hand() {
        for c in [Curve::A, Curve::B, Curve::C, Curve::D] {
            assert!((chi(0.2, c.alpha()) - 1.0).abs() < 1e-12);
        }
        let phi: f64 = 0.5 * (1.0 + 0.49 * 0.8 + 1.0);
        assert!((phi - 1.196).abs() < 1e-12);
        let by_hand = 1.0 / (phi + (phi * phi - 1.0).sqrt());
        assert!((chi(1.0, 0.49) - by_hand).abs() < 1e-12);
        assert!((by_hand - 0.540).abs() < 0.001, "{by_hand}");
        // λ_E of S 235 in Figure 7: 93.9.
        let le = std::f64::consts::PI * (210e9f64 / 235e6).sqrt();
        assert!((le - 93.9).abs() < 0.05, "{le}");
        // ε of Table 5b.
        assert!((epsilon(355e6) - 0.814).abs() < 0.001);
        // η and ω at their figures' points.
        assert!((eta(1.0) - 1.0).abs() < 1e-12);
        assert!((eta(0.0) - 1.75).abs() < 1e-12);
        assert!((eta(-0.5) - 2.35).abs() < 1e-12);
        assert!((eta(-1.0) - 2.35).abs() < 1e-12);
        assert!((omega(-1.0) - 0.4).abs() < 1e-12);
        assert!((omega(1.0) - 1.0).abs() < 1e-12);
        // Table 10: ρ = 1 up to 0.9 · 28.4 ε √k: an outstand of S 235 at
        // b/t 17 (k 0.426), an internal element of S 355 at 42 (k 4).
        assert!((rho(16.0, K_OUTSTAND, 235e6) - 1.0).abs() < 1e-12);
        assert!(rho(17.0, K_OUTSTAND, 235e6) < 1.0 && rho(16.6, K_OUTSTAND, 235e6) == 1.0);
        assert!(rho(41.0, K_INTERNAL, 355e6) == 1.0 && rho(42.0, K_INTERNAL, 355e6) < 1.0);
    }

    /// Classes by Tables 5a and 5b.
    #[test]
    fn classes_by_the_tables() {
        let fy = 355e6;
        let e = epsilon(fy);
        assert_eq!(Element::Outstand(9.0 * e).class(fy), 1);
        assert_eq!(Element::Outstand(9.5 * e).class(fy), 2);
        assert_eq!(Element::Outstand(14.0 * e).class(fy), 3);
        assert_eq!(Element::Outstand(14.1 * e).class(fy), 4);
        assert_eq!(Element::Internal(42.0 * e).class(fy), 3);
        assert_eq!(Element::Tube(91.0 * e * e).class(fy), 4);
    }

    /// A round tube's section by hand: σ = N/A + M/W, τ = 2V/A + T/(2A_m t),
    /// von Mises against f_y/γ_M1; a tube above 90 ε² is outside.
    #[test]
    fn pole_shaft_by_hand() {
        let shaft = crate::asce48::Shaft {
            sides: None,
            d: 0.5,
            t: 0.010,
            bend_radius: None,
        };
        let (fy, p, m, v, t) = (355e6, 50e3, 200e3, 20e3, 5e3);
        let (a, i) = shaft.area_inertia();
        let w = i / 0.25;
        // D/t 50: class 3 in S 355 (above 70 ε² = 46.4, within 90 ε²).
        let am = std::f64::consts::PI / 4.0 * (0.5f64 - 0.010).powi(2);
        let tau = 2.0 * v / a + t / (2.0 * am * 0.010);
        let sigma = p / a + m / w;
        let want = (sigma * sigma + 3.0 * tau * tau).sqrt() / (fy / GAMMA_M1);
        let (u, words) = shaft_usage(&shaft, p, m, v, t, fy, 1.0);
        assert!((u - want).abs() < 1e-12, "{u} {want} {words}");
        assert!(words.contains("class 3"), "{words}");
        let thin = crate::asce48::Shaft { t: 0.008, ..shaft };
        let (u, words) = shaft_usage(&thin, p, m, v, t, fy, 1.0);
        assert!(
            u.is_infinite() && words.contains("outside SIA 263"),
            "{words}"
        );
        use crate::pole::PoleCode;
        assert_eq!(PoleCode::parse(Some(NAME)), Ok(PoleCode::Sia263));
        assert_eq!(PoleCode::parse(None), Ok(PoleCode::Asce48));
        assert!(PoleCode::parse(Some("other")).is_err());
    }

    /// Table 17 by size; another size none.
    #[test]
    fn bolt_table() {
        assert_eq!(bolt_minimums(0.020), Some([0.030, 0.045, 0.025]));
        assert_eq!(standard_hole(0.016), Some(0.018));
        assert_eq!(bolt_minimums(0.018), None);
        let g = crate::design::Grade::parse("8.8");
        assert_eq!(shear_factor(g), (0.6, true));
        assert_eq!(
            shear_factor(crate::design::Grade::parse("10.9")),
            (0.5, true)
        );
        assert_eq!(shear_factor(None), (0.5, false));
    }
}
