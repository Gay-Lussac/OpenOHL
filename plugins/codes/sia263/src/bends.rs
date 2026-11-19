// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **A bent plate's bend** (the interface's `joint-check-bends`, 2026-11-19,
//! the owner's decision of that day): the plate's section along its bend
//! line — width w the bend's length, depth t the plate's thickness — under
//! what OpenOHL works out per case: the force N across the bend, the shear
//! V along it, the moment M about the bend line and M_in in the plate's
//! plane.
//!
//! - **The check**: SIA 263:2013, 4.3.5.4, formula (6) — the comparison
//!   stress of a plane state of stress no larger than f_y / γ_M1 anywhere
//!   in the section: σ² + 3 τ² ≤ (f_y / γ_M1)² (σ_y = 0 across the
//!   section), with the stresses distributed **elastically** (5.2.1, the
//!   EE method; 5.2.6: N/N_Rd + M/M_el,Rd + M_in/M_el,Rd): σ = N/A + M/W +
//!   M_in/W_in, W = w t²/6, W_in = t w²/6, τ = V/A taken at the same point
//!   — on the safe side: the plastic resistance a solid plate of class 1
//!   could have (5.1.3, W_pl) is not relied on in a cold-bent zone, and the
//!   peak normal stress and the shear are taken together. In compression
//!   the same (the bend is short across its section; the flats' own strut
//!   checks take their buckling). γ_M1 4.1.3 (or the parameter file's).
//! - **The inside radius**: SIA 263 5.7.1.2, Table 14 — where a weld is
//!   made within 5 t of a cold bend, the inside radius at least 1 t (t ≤
//!   3 mm), 1.5 t (3 < t ≤ 6), 2 t (6 < t ≤ 12), 3 t (12 < t ≤ 24), 10 t
//!   (t > 24). A weld on the bent plate (where, not known: it may be within
//!   the zone) with a smaller radius: NOT ON THE SAFE SIDE, said; without a
//!   weld the table does not apply (said where the radius is below it).

use crate::params::gamma_m1;

/// One bend as the interface gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Bend {
    /// The two flats, in words.
    pub part: String,
    /// Thickness, m.
    pub thickness: f64,
    /// Yield strength, Pa.
    pub fy: Option<f64>,
    /// The bend's length (the section's width), m.
    pub width: f64,
    /// Its inside radius, m.
    pub inner_radius: f64,
    /// A weld on either flat.
    pub welded: bool,
}

/// What acts on the section in one case: across (N, tension +), along
/// (N), about the bend line (N m), in the plate's plane (N m).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BendLoad {
    pub axial: f64,
    pub shear: f64,
    pub moment: f64,
    pub moment_in_plane: f64,
}

/// A bend's result: per case its usage and check, what governs, notes,
/// what is not on the safe side.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BendResult {
    pub usages: Vec<Option<(f64, String)>>,
    pub governs: String,
    pub notes: Vec<String>,
    pub not_safe: Vec<String>,
}

/// SIA 263:2013, 5.7.1.2, Table 14: the least inside radius of a cold
/// bend with a weld within 5 t of it, as a multiple of the thickness `t`
/// (m).
pub fn least_radius_welded(t: f64) -> f64 {
    let mm = t * 1000.0;
    if mm <= 3.0 {
        1.0
    } else if mm <= 6.0 {
        1.5
    } else if mm <= 12.0 {
        2.0
    } else if mm <= 24.0 {
        3.0
    } else {
        10.0
    }
}

/// The bend `b` in every case (`strengths`: each case's strength factor).
pub fn check_bend(b: &Bend, loads: &[BendLoad], strengths: &[f64]) -> BendResult {
    let mut out = BendResult::default();
    let (w, t) = (b.width, b.thickness);
    let k = least_radius_welded(t);
    if b.inner_radius < k * t - 1e-6 {
        let words = format!(
            "inside radius {:.0} mm ({:.1} t) below the {k} t SIA 263 5.7.1.2, Table 14 asks \
             where a weld is made within 5 t of the bend",
            b.inner_radius * 1000.0,
            b.inner_radius / t.max(1e-12)
        );
        if b.welded {
            out.not_safe.push(format!(
                "{words}: the bent plate has a weld (where, not known)"
            ));
        } else {
            out.notes.push(format!(
                "{words} (no weld on the bent plate: it does not apply)"
            ));
        }
    }
    let Some(fy) = b.fy else {
        out.not_safe
            .push("the bend's bending not checked (no yield strength)".into());
        out.usages = vec![None; strengths.len()];
        return out;
    };
    if w <= 0.0 || t <= 0.0 {
        out.not_safe
            .push("the bend's bending not checked (its section not known)".into());
        out.usages = vec![None; strengths.len()];
        return out;
    }
    let a = w * t;
    let wel = w * t * t / 6.0;
    let wel_in = t * w * w / 6.0;
    let fd = fy / gamma_m1();
    let mut worst: Option<f64> = None;
    for (n, strength) in strengths.iter().enumerate() {
        let Some(l) = loads.get(n) else {
            out.usages.push(None);
            continue;
        };
        let sigma = l.axial.abs() / a + l.moment.abs() / wel + l.moment_in_plane.abs() / wel_in;
        let tau = l.shear.abs() / a;
        let eq = (sigma * sigma + 3.0 * tau * tau).sqrt();
        let r = fd * strength;
        let u = if r > 0.0 { eq / r } else { f64::INFINITY };
        let words = format!(
            "the bend of {} ({:.0} × {:.0} mm): N {:.1} kN, M {:.2} kN m, M in its plane {:.2} kN m, \
             V {:.1} kN — σ = N/A + M/W_el + M_in/W_el,in {:.0} MPa, τ = V/A {:.0} MPa, √(σ² + 3τ²) \
             {:.0} MPa of f_y/γ_M1 {:.0} MPa (SIA 263:2013 4.3.5.4, formula (6), elastic: 5.2.1, \
             5.2.6; γ_M1 {})",
            b.part,
            w * 1000.0,
            t * 1000.0,
            l.axial / 1000.0,
            l.moment / 1000.0,
            l.moment_in_plane / 1000.0,
            l.shear / 1000.0,
            sigma / 1e6,
            tau / 1e6,
            eq / 1e6,
            fd / 1e6,
            gamma_m1()
        );
        if worst.is_none_or(|x| crate::check::usage_order(u, x).is_gt()) {
            worst = Some(u);
            out.governs = words.clone();
        }
        out.usages.push(Some((u, words)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bend() -> Bend {
        Bend {
            part: "P2/P1".into(),
            thickness: 0.010,
            fy: Some(355e6),
            width: 0.2,
            inner_radius: 0.02,
            welded: false,
        }
    }

    /// By hand, γ_M1 = 1.05 (4.1.3): 200 × 10 mm, f_y 355 MPa, f_y/γ_M1 =
    /// 338.10 MPa. N 100 kN with M 0.5 kN m: 50 + 150 = 200 MPa, 59.15 %;
    /// V 100 kN alone: √3 × 50 = 86.60 MPa, 25.61 %.
    #[test]
    fn a_bend_by_hand() {
        let l = |axial, shear, moment| BendLoad {
            axial,
            shear,
            moment,
            moment_in_plane: 0.0,
        };
        let r = check_bend(
            &bend(),
            &[l(100e3, 0.0, 500.0), l(0.0, 100e3, 0.0)],
            &[1.0, 1.0],
        );
        let u: Vec<f64> = r.usages.iter().map(|x| x.as_ref().unwrap().0).collect();
        let fd = 355.0 / 1.05;
        assert!((u[0] - 200.0 / fd).abs() < 1e-9, "{u:?}");
        assert!((u[1] - 3f64.sqrt() * 50.0 / fd).abs() < 1e-9, "{u:?}");
        assert!(r.governs.contains("4.3.5.4"), "{}", r.governs);
        assert!(r.not_safe.is_empty() && r.notes.is_empty(), "{r:?}");
    }

    /// Table 14 (5.7.1.2): a 10 mm plate bent to 15 mm (1.5 t) is below
    /// the 2 t asked with a weld within 5 t: said not on the safe side
    /// where the plate has a weld, a note without one; 30 mm needs 10 t.
    #[test]
    fn table_14_radius() {
        assert_eq!(least_radius_welded(0.003), 1.0);
        assert_eq!(least_radius_welded(0.006), 1.5);
        assert_eq!(least_radius_welded(0.010), 2.0);
        assert_eq!(least_radius_welded(0.020), 3.0);
        assert_eq!(least_radius_welded(0.030), 10.0);
        let tight = Bend {
            inner_radius: 0.015,
            ..bend()
        };
        let r = check_bend(&tight, &[], &[]);
        assert!(r.not_safe.is_empty() && r.notes.len() == 1, "{r:?}");
        let r = check_bend(
            &Bend {
                welded: true,
                ..tight
            },
            &[],
            &[],
        );
        assert!(r.not_safe.iter().any(|w| w.contains("Table 14")), "{r:?}");
    }
}
