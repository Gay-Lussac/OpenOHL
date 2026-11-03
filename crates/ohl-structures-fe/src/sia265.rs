// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Wood poles by **SIA 265:2012** (*Construction en bois / Holzbau*,
//! Swiss Society of Engineers and Architects; owner 2026-11-03): a solid
//! round wood pole with its strength class (`strength_class`, e.g.
//! `"C24"`) and no capacity given. Built, in active development, not
//! approved yet; clause, table and formula numbers are the standard's
//! (`docs/theory/sia265.md`).
//!
//! - Round timber as solid timber (3.3.1); the design values of Table 6
//!   (they hold γ_M/η_M = 1.7 of Table 1 for round and solid timber),
//!   times η_w of the moisture class (Table 4: 1, 0.80, 0.60) and η_t = 1
//!   (2.2.6: 1.4 only for shock-like actions); the shear values halved for
//!   pieces with pith (Table 6, note 6) — a pole is one.
//! - Buckling (4.2.8): k_c by (30), (31) with β_c = 0.2 (solid timber),
//!   λ_rel by (34) with f_c,0,k and E_0,05 of Table 43.
//! - Bending with compression (37): σ_c,0,d/(k_c f_c,0,d) + σ_m,d/(k_m
//!   f_m,d) ≤ 1, k_m = 1 (a round section does not tip); in tension
//!   σ_m,d/f_m,d (no tension value is allowed in tension for C16, note 2).
//! - Shear and torsion: τ = 4V/(3A) + 16T/(πD³) ≤ f_v,d.

/// The method's name in files and results.
pub const NAME: &str = "sia265-2012";

/// The standard's name in results.
pub const TITLE: &str = "SIA 265:2012";

/// A strength class: its design values (Table 6, η_w = η_t = 1) and the
/// characteristic compression and 5 % modulus (Table 43), MPa.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrengthClass {
    /// Its name.
    pub name: &'static str,
    /// f_m,d.
    pub fm_d: f64,
    /// f_c,0,d.
    pub fc0_d: f64,
    /// f_v,d.
    pub fv_d: f64,
    /// f_c,0,k (Table 43).
    pub fc0_k: f64,
    /// E_0,05 (Table 43).
    pub e005: f64,
}

/// The solid timber classes of Tables 6 and 43, MPa.
const CLASSES: [StrengthClass; 4] = [
    StrengthClass {
        name: "C16",
        fm_d: 9.5,
        fc0_d: 9.5,
        fv_d: 1.5,
        fc0_k: 17.0,
        e005: 5300.0,
    },
    StrengthClass {
        name: "C24",
        fm_d: 14.0,
        fc0_d: 12.0,
        fv_d: 1.5,
        fc0_k: 21.0,
        e005: 7300.0,
    },
    StrengthClass {
        name: "C30",
        fm_d: 17.5,
        fc0_d: 13.5,
        fv_d: 1.5,
        fc0_k: 23.0,
        e005: 8000.0,
    },
    StrengthClass {
        name: "D30",
        fm_d: 17.0,
        fc0_d: 13.0,
        fv_d: 2.0,
        fc0_k: 23.0,
        e005: 8300.0,
    },
];

/// The class of a name ("C24", any case).
pub fn class(name: &str) -> Option<StrengthClass> {
    CLASSES
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(name.trim()))
        .copied()
}

/// The names of the classes.
pub fn class_names() -> Vec<&'static str> {
    CLASSES.iter().map(|c| c.name).collect()
}

/// η_w of a moisture class on the resistance (Table 4; class 1: 1).
pub fn eta_w(moisture: u8) -> Option<f64> {
    match moisture {
        1 => Some(1.0),
        2 => Some(0.80),
        3 => Some(0.60),
        _ => None,
    }
}

/// k_c by (30), (31) with β_c = 0.2: k = 0.5 [1 + β_c (λ_rel − 0.3) +
/// λ_rel²], k_c = 1/(k + √(k² − λ_rel²)) ≤ 1 (λ_rel at least 0.3).
pub fn kc(lambda_rel: f64) -> f64 {
    let l = lambda_rel.max(0.3);
    let k = 0.5 * (1.0 + 0.2 * (l - 0.3) + l * l);
    (1.0 / (k + (k * k - l * l).max(0.0).sqrt())).min(1.0)
}

/// λ_rel by (34): λ/π √(f_c,0,k/E_0,05).
pub fn lambda_rel(lambda: f64, c: &StrengthClass) -> f64 {
    lambda / std::f64::consts::PI * (c.fc0_k / c.e005).sqrt()
}

/// The usage of a solid round wood section of diameter `d` (m) at a
/// height: axial force `p` (compression positive), moment `m`, shear `v`,
/// torsion `t` (N, N·m), the pole's k_c, the class, η_w, the load case's
/// strength factor.
#[allow(clippy::too_many_arguments)]
pub fn round_usage(
    d: f64,
    p: f64,
    m: f64,
    v: f64,
    t: f64,
    kc: f64,
    c: &StrengthClass,
    eta_w: f64,
    strength: f64,
) -> (f64, String) {
    let pi = std::f64::consts::PI;
    let a = pi * d * d / 4.0;
    let w = pi * d.powi(3) / 32.0;
    let mpa = 1e6 * eta_w * strength;
    let (fm, fc, fv) = (c.fm_d * mpa, c.fc0_d * mpa, c.fv_d * mpa / 2.0);
    let sm = m.abs() / w;
    let sc = p.max(0.0) / a;
    let tau = 4.0 * v.abs() / (3.0 * a) + 16.0 * t.abs() / (pi * d.powi(3));
    let bend = if sc > 0.0 {
        (
            sc / (kc * fc) + sm / fm,
            format!(
                "σ_c {:.2} MPa / (k_c {kc:.3} · f_c,0,d {:.2}) + σ_m {:.2} / f_m,d {:.2} (37)",
                sc / 1e6,
                fc / 1e6,
                sm / 1e6,
                fm / 1e6
            ),
        )
    } else {
        (
            sm / fm,
            format!("σ_m {:.2} MPa of f_m,d {:.2}", sm / 1e6, fm / 1e6),
        )
    };
    let shear = (
        tau / fv,
        format!(
            "τ {:.2} MPa of f_v,d/2 {:.2} (pith, Table 6 note 6)",
            tau / 1e6,
            fv / 1e6
        ),
    );
    if shear.0 > bend.0 || shear.0.is_nan() {
        shear
    } else {
        bend
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tables as printed, k_c at its figure's ends, a section by hand.
    #[test]
    fn wood_by_hand() {
        let c = class("c24").unwrap();
        assert_eq!(
            (c.fm_d, c.fc0_d, c.fc0_k, c.e005),
            (14.0, 12.0, 21.0, 7300.0)
        );
        assert!(class("C40").is_none());
        assert_eq!(eta_w(3), Some(0.60));
        assert!((kc(0.3) - 1.0).abs() < 1e-12);
        // λ_rel 1: k = 0.5 (1 + 0.14 + 1) = 1.07.
        let k: f64 = 1.07;
        assert!((kc(1.0) - 1.0 / (k + (k * k - 1.0).sqrt())).abs() < 1e-12);
        // (35), the simplification for softwood, λ/(18π): within 4 % of (34)
        // for C24.
        let simple = 100.0 / (18.0 * std::f64::consts::PI);
        assert!((lambda_rel(100.0, &c) / simple - 1.0).abs() < 0.04);
        let (d, p, m) = (0.25, 20e3, 30e3);
        let a = std::f64::consts::PI * d * d / 4.0;
        let w = std::f64::consts::PI * d * d * d / 32.0;
        let k = 0.5;
        let want = p / a / (k * 12e6 * 0.6) + m / w / (14e6 * 0.6);
        let (u, words) = round_usage(d, p, m, 0.0, 0.0, k, &c, 0.6, 1.0);
        assert!((u - want).abs() < 1e-12, "{u} {want} {words}");
        assert!(words.contains("(37)"));
    }
}
