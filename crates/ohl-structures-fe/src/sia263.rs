// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **SIA 263:2013** (*Construction en acier / Stahlbau*) for the shafts of
//! steel poles ([`shaft_usage`], `[[pole]] code = "sia263-2013"`; owner
//! 2026-11-03). Built, in active development, not approved yet.
//!
//! The members of lattice structures are checked by SIA 263 in the
//! bundled plugin `sia263` (`plugins/codes/sia263`: moved out of the core
//! on 2026-11-18, owner 2026-11-17; a file's `[analysis] capacity =
//! "sia263-2013"` names its method,
//! [`crate::model::BUNDLED_ALIASES`]). The pole codes stay in the core
//! until a pole export exists in the plugin interface. Clause, table and
//! formula numbers are the standard's; the theory page
//! (`docs/theory/sia263.md`) explains every step.

/// The method's name in files and results.
pub const NAME: &str = "sia263-2013";

/// The standard's name in results.
pub const TITLE: &str = "SIA 263:2013";

/// Resistance factor of members (4.1.3).
pub const GAMMA_M1: f64 = 1.05;

/// The reference yield strength of ε = √(235/f_y), Pa (Table 5b).
const FY_REF: f64 = 235e6;

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

/// k of an internal element in uniform compression (Table 9, ψ = 1).
const K_INTERNAL: f64 = 4.0;

/// The class limits of an element in uniform compression, b/t over ε:
/// classes 1, 2, 3 (Table 5a); a tube's D/t over ε².
#[derive(Debug, Clone, Copy, PartialEq)]
enum Element {
    /// Supported at both edges (Table 5a, compression: 33, 38, 42).
    Internal(f64),
    /// A tube's D/t (Table 5a, tubes: 50, 70, 90 ε²).
    Tube(f64),
}

impl Element {
    /// The section class of this element at yield strength `fy`.
    fn class(self, fy: f64) -> u8 {
        let e = epsilon(fy);
        let (x, limits) = match self {
            Element::Internal(b_t) => (b_t / e, [33.0, 38.0, 42.0]),
            Element::Tube(d_t) => (d_t / (e * e), [50.0, 70.0, 90.0]),
        };
        limits
            .iter()
            .position(|l| x <= *l)
            .map_or(4, |k| k as u8 + 1)
    }
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
}
